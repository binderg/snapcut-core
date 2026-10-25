//! Media I/O.
//!
//! Phase 1 drives the `ffmpeg` and `ffprobe` binaries through ffmpeg-sidecar:
//! `media.probe` is `ffprobe -print_format json -show_streams -show_format`
//! deserialized into our own structs. Phase 2 adds an in-process decoder pool
//! (ffmpeg-the-third) for GOP-aware seeks into the compositor.
//!
//! Media is addressed by URI (`file://` now, `s3://` later), never by bare path.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;
use ve_model::meta::{AudioInfo, ColorInfo, MediaInfo, VideoInfo};
use ve_model::time::Q;
use ve_model::{Rate, RationalTime};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("ffprobe not found (tried {}); install FFmpeg and put ffprobe on PATH", .0.display())]
    FfprobeMissing(PathBuf),
    #[error("{} is not readable media: {message}", .path.display())]
    NotMedia { path: PathBuf, message: String },
    #[error("{} has no video or audio stream", .0.display())]
    NoStreams(PathBuf),
    #[error("{}: {source}", .path.display())]
    Io { path: PathBuf, source: io::Error },
}

type Result<T> = std::result::Result<T, MediaError>;

/// True when an `ffmpeg` binary is reachable (PATH, or next to the engine).
pub fn ffmpeg_available() -> bool {
    ffmpeg_sidecar::command::ffmpeg_is_installed()
}

/// Content hash used in the asset table.
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// blake3 of a file's contents, streamed so multi-GB footage never sits in memory.
pub fn hash_file(path: &Path) -> Result<String> {
    let io_err = |source| MediaError::Io { path: path.to_owned(), source };
    let file = File::open(path).map_err(io_err)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file).map_err(io_err)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Absolute `file://` URL for a local path (`file:///C:/footage/IMG%204410.MOV`).
pub fn file_url(path: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(path).map_err(|source| MediaError::Io { path: path.to_owned(), source })?;
    // Windows canonical paths start with `\\?\`, which file URLs cannot carry.
    let abs = match abs.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => abs,
    };
    url::Url::from_file_path(&abs).map(String::from).map_err(|()| MediaError::Io {
        path: abs,
        source: io::Error::other("path cannot be expressed as a file:// URL"),
    })
}

/// One probed file: the compact summary we store, plus ffprobe's own JSON.
#[derive(Debug, Clone)]
pub struct Probe {
    pub media: MediaInfo,
    pub ffprobe: Value,
}

/// Run ffprobe on a local file.
pub fn probe(path: &Path) -> Result<Probe> {
    if !path.is_file() {
        return Err(MediaError::Io { path: path.to_owned(), source: io::ErrorKind::NotFound.into() });
    }
    let exe = ffmpeg_sidecar::ffprobe::ffprobe_path();
    let out = Command::new(&exe)
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => MediaError::FfprobeMissing(exe.clone()),
            _ => MediaError::Io { path: exe.clone(), source: e },
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // ffprobe prefixes its reason with the file name; keep the last line, which says why.
        let message = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ffprobe failed").trim();
        let message = message.rsplit_once(": ").map_or(message, |(_, why)| why).to_owned();
        return Err(MediaError::NotMedia { path: path.to_owned(), message });
    }
    let ffprobe: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| MediaError::NotMedia { path: path.to_owned(), message: format!("unreadable ffprobe output: {e}") })?;
    let media = summarize(&ffprobe).ok_or_else(|| MediaError::NoStreams(path.to_owned()))?;
    Ok(Probe { media, ffprobe })
}

#[derive(Deserialize)]
struct FfProbe {
    #[serde(default)]
    streams: Vec<FfStream>,
    #[serde(default)]
    format: FfFormat,
}

#[derive(Deserialize, Default)]
struct FfFormat {
    duration: Option<String>,
    format_name: Option<String>,
    size: Option<String>,
}

#[derive(Deserialize)]
struct FfStream {
    codec_type: Option<String>,
    #[serde(default)]
    codec_name: String,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    time_base: Option<String>,
    duration_ts: Option<i64>,
    nb_frames: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    pix_fmt: Option<String>,
    bits_per_raw_sample: Option<String>,
    color_range: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    #[serde(default)]
    side_data_list: Vec<SideData>,
    #[serde(default)]
    tags: Tags,
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Deserialize)]
struct SideData {
    rotation: Option<f64>,
}

#[derive(Deserialize, Default)]
struct Tags {
    rotate: Option<String>,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

/// Compact summary of ffprobe JSON. `None` when there is no usable stream.
pub fn summarize(ffprobe: &Value) -> Option<MediaInfo> {
    let p: FfProbe = serde_json::from_value(ffprobe.clone()).ok()?;
    let kind = |s: &&FfStream, k: &str| s.codec_type.as_deref() == Some(k);
    // Cover art in audio files shows up as a one-frame "video" stream.
    let v = p.streams.iter().find(|s| kind(s, "video") && s.disposition.attached_pic == 0);
    let a = p.streams.iter().find(|s| kind(s, "audio"));

    let still = v.is_some_and(|s| is_still(&p.format, s));
    let video = v.and_then(|s| {
        let r = s.r_frame_rate.as_deref().and_then(parse_ratio);
        let avg = s.avg_frame_rate.as_deref().and_then(parse_ratio);
        Some(VideoInfo {
            codec: s.codec_name.clone(),
            width: s.width?,
            height: s.height?,
            rate: nominal_rate(r, avg)?,
            rotation: rotation(s),
            vfr: !still && matches!((r, avg), (Some(r), Some(avg)) if r != avg),
            still,
            color: color(s),
        })
    });
    let audio = a.and_then(|s| {
        Some(AudioInfo {
            codec: s.codec_name.clone(),
            sample_rate: s.sample_rate.as_deref()?.parse().ok()?,
            channels: s.channels.unwrap_or(0),
        })
    });

    let (rate, stream) = match (&video, &audio) {
        (Some(vi), _) => (vi.rate, v),
        (None, Some(ai)) => (Rate::integer(i64::from(ai.sample_rate)), a),
        (None, None) => return None,
    };
    let seconds = if still { Some(Q::from_integer(0)) } else { None }
        .or_else(|| stream.and_then(|s| Some(Q::from_integer(s.duration_ts?) * parse_ratio(s.time_base.as_deref()?)?))
        .or_else(|| {
            let d = p.format.duration.as_deref()?;
            ve_model::timecode::parse(&format!("{d}s"), Rate::integer(1)).ok().map(|t| t.value)
        }))
        .unwrap_or(Q::from_integer(0));
    Some(MediaInfo {
        duration: RationalTime { value: seconds * rate.as_ratio(), rate },
        video,
        audio,
        size_bytes: p.format.size.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0),
    })
}

/// A still image: ffmpeg's image demuxers (`png_pipe`, `jpeg_pipe`,
/// `webp_pipe`, `image2`, ...), or a GIF with a single frame. Animated GIFs
/// are video.
fn is_still(f: &FfFormat, s: &FfStream) -> bool {
    match f.format_name.as_deref() {
        Some(name) if name.ends_with("_pipe") || name == "image2" => true,
        Some("gif") => s.nb_frames.as_deref() == Some("1"),
        _ => false,
    }
}

/// ffprobe writes rates and time bases as `30000/1001`; `0/0` means unknown.
fn parse_ratio(s: &str) -> Option<Q> {
    let (n, d) = s.split_once('/')?;
    let (n, d): (i64, i64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0 && d > 0).then(|| Q::new(n, d))
}

const STANDARD_RATES: [(i64, i64); 12] = [
    (24_000, 1001),
    (24, 1),
    (25, 1),
    (30_000, 1001),
    (30, 1),
    (48, 1),
    (50, 1),
    (60_000, 1001),
    (60, 1),
    (100, 1),
    (120_000, 1001),
    (120, 1),
];

/// The rate the footage was shot at. `r_frame_rate` is the capture rate and
/// stays right on variable-frame-rate phone video, where the average drifts
/// (`2040/89`). If it is not a standard rate (some containers report a
/// timebase like 90000), use the average snapped to the nearest standard
/// rate within 0.5%, else the average as is.
fn nominal_rate(r: Option<Q>, avg: Option<Q>) -> Option<Rate> {
    let standard = |q: Q| STANDARD_RATES.iter().map(|&(n, d)| Q::new(n, d)).find(|s| *s == q);
    if let Some(s) = r.and_then(standard) {
        return Some(Rate::new(*s.numer(), *s.denom()));
    }
    let q = avg.or(r)?;
    let close = STANDARD_RATES.iter().map(|&(n, d)| Q::new(n, d)).find(|s| {
        let diff = q - *s;
        diff * Q::from_integer(200) < *s && -diff * Q::from_integer(200) < *s
    });
    let q = close.unwrap_or(q);
    Some(Rate::new(*q.numer(), *q.denom()))
}

/// The stream's color description. `None` when ffprobe gives no pixel format.
fn color(s: &FfStream) -> Option<ColorInfo> {
    let pix = s.pix_fmt.clone().filter(|p| !p.is_empty())?;
    // ffprobe says `unknown` / `unspecified` / `reserved` for fields the file leaves open.
    let known = |v: &Option<String>| v.clone().filter(|v| !matches!(v.as_str(), "" | "unknown" | "unspecified" | "reserved"));
    let range = known(&s.color_range).map(|r| match r.as_str() {
        "tv" | "mpeg" => "limited".to_owned(),
        "pc" | "jpeg" => "full".to_owned(),
        _ => r,
    });
    let transfer = known(&s.color_transfer);
    let dynamic_range = match transfer.as_deref() {
        Some("smpte2084") => "hdr10",
        Some("arib-std-b67") => "hlg",
        _ => "sdr",
    };
    let bit_depth = s
        .bits_per_raw_sample
        .as_deref()
        .and_then(|b| b.parse::<u8>().ok())
        .filter(|&b| b > 0)
        .unwrap_or_else(|| pixel_bit_depth(&pix));
    Some(ColorInfo {
        chroma: chroma(&pix),
        bit_depth,
        range,
        primaries: known(&s.color_primaries),
        transfer,
        matrix: known(&s.color_space),
        dynamic_range: dynamic_range.into(),
        pixel_format: pix,
    })
}

/// Bits per component from a pixel format name: `yuv420p10le` is 10, `rgb48be` 16, else 8.
fn pixel_bit_depth(pix: &str) -> u8 {
    for (tag, bits) in [("p16", 16), ("p14", 14), ("p12", 12), ("p10", 10), ("p9", 9), ("48", 16), ("64", 16), ("p010", 10), ("p016", 16)] {
        if pix.contains(tag) {
            return bits;
        }
    }
    8
}

fn chroma(pix: &str) -> Option<String> {
    let c = if pix.starts_with("yuv420") || pix.starts_with("yuvj420") || pix.starts_with("nv12") || pix.starts_with("nv21") || pix.starts_with("p010") || pix.starts_with("p016") {
        "4:2:0"
    } else if pix.starts_with("yuv422") || pix.starts_with("yuvj422") || pix.starts_with("nv16") || pix.starts_with("p210") {
        "4:2:2"
    } else if pix.starts_with("yuv444") || pix.starts_with("yuvj444") || pix.starts_with("p410") {
        "4:4:4"
    } else if pix.starts_with("rgb") || pix.starts_with("bgr") || pix.starts_with("gbr") || pix.starts_with("argb") || pix.starts_with("abgr") {
        "rgb"
    } else if pix.starts_with("gray") {
        "gray"
    } else {
        return None;
    };
    Some(c.into())
}

/// Display rotation in whole degrees, normalized to 0..360.
fn rotation(s: &FfStream) -> u16 {
    let deg = s
        .side_data_list
        .iter()
        .find_map(|d| d.rotation)
        .or_else(|| s.tags.rotate.as_deref().and_then(|r| r.parse().ok()))
        .unwrap_or(0.0);
    // Rotations are multiples of 90 in practice; round any float noise away.
    let deg = (deg.round() as i64).rem_euclid(360);
    u16::try_from(deg).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn video_stream(r: &str, avg: &str) -> Value {
        json!({
            "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
            "r_frame_rate": r, "avg_frame_rate": avg, "time_base": "1/600", "duration_ts": 1780
        })
    }

    #[test]
    fn reads_color_description() {
        let mut s = video_stream("30/1", "30/1");
        s["pix_fmt"] = json!("yuv420p10le");
        s["color_range"] = json!("tv");
        s["color_primaries"] = json!("bt2020");
        s["color_transfer"] = json!("arib-std-b67");
        s["color_space"] = json!("bt2020nc");
        let c = summarize(&json!({"streams": [s]})).unwrap().video.unwrap().color.unwrap();
        assert_eq!((c.bit_depth, c.chroma.as_deref(), c.range.as_deref()), (10, Some("4:2:0"), Some("limited")));
        assert_eq!((c.primaries.as_deref(), c.matrix.as_deref(), c.dynamic_range.as_str()), (Some("bt2020"), Some("bt2020nc"), "hlg"));

        let mut s = video_stream("30/1", "30/1");
        s["pix_fmt"] = json!("yuvj420p");
        s["color_range"] = json!("pc");
        s["color_transfer"] = json!("unknown");
        let c = summarize(&json!({"streams": [s]})).unwrap().video.unwrap().color.unwrap();
        assert_eq!((c.bit_depth, c.range.as_deref(), c.transfer, c.dynamic_range.as_str()), (8, Some("full"), None, "sdr"));
    }

    #[test]
    fn vfr_phone_video_keeps_capture_rate() {
        let m = summarize(&json!({"streams": [video_stream("30/1", "2040/89")], "format": {"size": "88538"}})).unwrap();
        let v = m.video.unwrap();
        assert_eq!(v.rate, Rate::integer(30));
        assert!(v.vfr);
        // 1780/600 s at 30 fps is exactly 89 frames.
        assert_eq!(m.duration.value, Q::from_integer(89));
        assert_eq!(m.size_bytes, 88_538);
    }

    #[test]
    fn odd_timebase_rate_falls_back_to_snapped_average() {
        let m = summarize(&json!({"streams": [video_stream("90000/1", "29999/1001")]})).unwrap();
        assert_eq!(m.video.unwrap().rate, Rate::new(30_000, 1001));
    }

    #[test]
    fn rotation_from_side_data_or_tag() {
        let mut s = video_stream("30/1", "30/1");
        s["side_data_list"] = json!([{"side_data_type": "Display Matrix", "rotation": -90}]);
        let m = summarize(&json!({"streams": [s]})).unwrap();
        assert_eq!(m.video.unwrap().rotation, 270);
        let mut s = video_stream("30/1", "30/1");
        s["tags"] = json!({"rotate": "90"});
        assert_eq!(summarize(&json!({"streams": [s]})).unwrap().video.unwrap().rotation, 90);
    }

    #[test]
    fn audio_only_and_cover_art() {
        let audio = json!({"codec_type": "audio", "codec_name": "mp3", "sample_rate": "44100", "channels": 2,
                           "time_base": "1/44100", "duration_ts": 66150});
        let cover = json!({"codec_type": "video", "codec_name": "mjpeg", "width": 600, "height": 600,
                           "r_frame_rate": "90000/1", "avg_frame_rate": "0/0", "disposition": {"attached_pic": 1}});
        let m = summarize(&json!({"streams": [cover, audio]})).unwrap();
        assert!(m.video.is_none());
        assert_eq!(m.duration.rate, Rate::integer(44_100));
        assert_eq!(m.duration.seconds(), Q::new(3, 2));
    }

    #[test]
    fn falls_back_to_format_duration() {
        let mut s = video_stream("25/1", "25/1");
        s.as_object_mut().unwrap().remove("duration_ts");
        let m = summarize(&json!({"streams": [s], "format": {"duration": "4.000000"}})).unwrap();
        assert_eq!(m.duration.value, Q::from_integer(100));
    }

    #[test]
    fn images_are_stills() {
        let png = json!({"codec_type": "video", "codec_name": "png", "width": 400, "height": 300,
                         "r_frame_rate": "25/1", "avg_frame_rate": "0/0", "pix_fmt": "rgba"});
        let m = summarize(&json!({"streams": [png], "format": {"format_name": "png_pipe"}})).unwrap();
        let v = m.video.unwrap();
        assert!(v.still && !v.vfr);
        assert_eq!(m.duration.value, Q::from_integer(0));
        // One-frame GIF is a still; an animated one is video.
        let gif = |n: &str| json!({"streams": [{"codec_type": "video", "codec_name": "gif", "width": 4, "height": 4,
            "r_frame_rate": "25/1", "avg_frame_rate": "25/1", "nb_frames": n, "time_base": "1/100", "duration_ts": 100}],
            "format": {"format_name": "gif"}});
        assert!(summarize(&gif("1")).unwrap().video.unwrap().still);
        assert!(!summarize(&gif("25")).unwrap().video.unwrap().still);
    }

    #[test]
    fn no_streams_is_none() {
        assert!(summarize(&json!({"streams": [{"codec_type": "data"}]})).is_none());
    }
}
