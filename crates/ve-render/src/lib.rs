//! Rendering.
//!
//! 1. Resolver ([`plan`]): for a timeline range, the clips per track with
//!    their media positions after speed and trims.
//! 2. Phase 1 backend ([`graph`]): compile the plan into one FFmpeg
//!    `-filter_complex` graph (fps conform, trim/atrim, setpts/atempo,
//!    scale/pad or crop, concat, overlay, amix, afade crossfades) and run
//!    the `ffmpeg` binary ([`ffmpeg`]). `frame` is the same graph over a
//!    one-frame range.
//! 3. Phase 2 backend: wgpu compositor ([`gpu`], timeline frames in
//!    [`compose`]) running WGSL effects and transitions. [`compose::encode`]
//!    pipes composited frames into the same FFmpeg encoder settings, with
//!    audio from the Phase 1 graph. [`needs_gpu`] says when a range needs it.
//!
//! Determinism rule: a frame is a pure function of (project, t). That is what
//! makes the frame cache safe.

pub mod afx;
pub mod control;
pub mod ffmpeg;
pub mod fx;
pub mod compose;
pub mod decoder;
pub mod gpu;
pub mod graph;
pub mod layers;
pub mod plan;
pub mod source;
pub mod text;

use std::path::Path;

use ve_model::Timeline;
use ve_model::time::Q;

pub use graph::{Canvas, Fit, Graph};
pub use plan::{Plan, Span, local_path, plan, timeline_end};
pub use source::{AudioSource, audio_filter_log, decode_audio};

#[derive(Debug, Clone, thiserror::Error)]
pub enum RenderError {
    /// The request itself is wrong (an empty or out-of-range span).
    #[error("{0}")]
    Invalid(String),
    /// Media the timeline needs is missing or unreadable.
    #[error("{0}")]
    Media(String),
    #[error("{message}{}", if stderr.is_empty() { String::new() } else { format!(":\n{stderr}") })]
    Ffmpeg { message: String, stderr: String },
    #[error("{0}")]
    Io(String),
    /// The job running this render was cancelled.
    #[error("cancelled")]
    Cancelled,
}

type Result<T> = std::result::Result<T, RenderError>;

/// Default audio crossfade at cuts: 20 ms, short enough to be inaudible as a
/// fade and long enough to remove clicks.
pub fn default_crossfade() -> Q {
    Q::new(1, 50)
}

/// The project's output settings as a canvas.
pub fn project_canvas(tl: &Timeline) -> Canvas {
    let (width, height, sample_rate) = tl.project().map_or((1920, 1080, 48_000), |p| (p.output.width, p.output.height, p.output.sample_rate));
    Canvas { width, height, rate: tl.output_rate(), sample_rate, fit: Fit::Fit }
}

/// `[from, to)` with defaults: the timeline start and end. Snapped to frames.
pub fn span_or_all(tl: &Timeline, from: Option<Q>, to: Option<Q>) -> Result<Span> {
    let end = timeline_end(tl);
    if end <= Q::from_integer(0) {
        return Err(RenderError::Invalid("the timeline is empty; insert something first".into()));
    }
    let r = tl.output_rate().as_ratio();
    let snap = |t: Q| (t * r).round() / r;
    let a = snap(from.unwrap_or(Q::from_integer(0)));
    let b = snap(to.unwrap_or(end)).min(end);
    if a < Q::from_integer(0) || a >= b {
        return Err(RenderError::Invalid(format!(
            "range is empty: it must start before the timeline end ({} frames)",
            (end * r).to_integer()
        )));
    }
    Ok(Span::new(a, b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
}

impl ImageFormat {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    fn args(self) -> Vec<String> {
        let a: &[&str] = match self {
            Self::Jpeg => &["-c:v", "mjpeg", "-q:v", "3", "-pix_fmt", "yuvj420p"],
            Self::Png => &["-c:v", "png", "-pix_fmt", "rgb24"],
        };
        a.iter().map(|s| (*s).to_owned()).collect()
    }
}

pub struct Image {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
}

fn strings(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// One frame at timeline time `at` (snapped down to its frame), `width` pixels wide.
pub fn frame(tl: &Timeline, at: Q, width: u32, format: ImageFormat, work: &Path) -> Result<Image> {
    let canvas = project_canvas(tl).scaled_to_width(width);
    let r = canvas.rate.as_ratio();
    let at = (at * r).floor() / r;
    let end = timeline_end(tl);
    if at < Q::from_integer(0) || at >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    let p = plan(tl, Span::new(at, at + Q::from_integer(1) / r))?;
    let mut g = Graph::new();
    let v = g.video(&p, &canvas);
    let mut out = strings(&["-map", &format!("[{v}]"), "-frames:v", "1", "-f", "image2pipe"]);
    out.extend(format.args());
    out.push("-".into());
    let o = ffmpeg::run(&g, &out, work, false)?;
    Ok(Image { bytes: o.stdout, width: canvas.width, height: canvas.height, warnings: p.warnings })
}

pub struct Sheet {
    pub image: Image,
    pub columns: u32,
    pub rows: u32,
    /// Timeline time of each cell, row by row.
    pub times: Vec<Q>,
}

/// `count` evenly spaced frames of `span` in a grid, each `cell_width` wide.
pub fn contact_sheet(tl: &Timeline, span: Span, count: u32, columns: Option<u32>, cell_width: u32, format: ImageFormat, work: &Path) -> Result<Sheet> {
    if !(1..=100).contains(&count) {
        return Err(RenderError::Invalid("count must be 1 to 100".into()));
    }
    let canvas = project_canvas(tl).scaled_to_width(cell_width);
    let r = canvas.rate.as_ratio();
    let cols = columns.unwrap_or_else(|| (f64::from(count).sqrt().ceil()) as u32).clamp(1, count);
    let rows = count.div_ceil(cols);
    let step = span.duration() / Q::from_integer(i64::from(count));
    let mut times: Vec<Q> = (0..count)
        .map(|k| {
            let t = span.start + step * Q::new(2 * i64::from(k) + 1, 2);
            (t * r).floor() / r
        })
        .collect();
    times.dedup();
    let mut g = Graph::new();
    let mut warnings = Vec::new();
    let mut cells = Vec::new();
    for &t in &times {
        let p = plan(tl, Span::new(t, t + Q::from_integer(1) / r))?;
        warnings.extend(p.warnings.iter().cloned());
        cells.push(g.video(&p, &canvas));
    }
    warnings.sort();
    warnings.dedup();
    let n = cells.len();
    let ins: String = cells.iter().map(|c| format!("[{c}]")).collect();
    let pad = 4;
    g.chain(format!("{ins}concat=n={n}:v=1:a=0,tile=layout={cols}x{rows}:nb_frames={n}:padding={pad}:margin={pad}:color=black[sheet]"));
    let mut out = strings(&["-map", "[sheet]", "-frames:v", "1", "-f", "image2pipe"]);
    out.extend(format.args());
    out.push("-".into());
    let o = ffmpeg::run(&g, &out, work, false)?;
    let width = cols * canvas.width + (cols - 1) * pad + 2 * pad;
    let height = rows * canvas.height + (rows - 1) * pad + 2 * pad;
    Ok(Sheet { image: Image { bytes: o.stdout, width, height, warnings }, columns: cols, rows, times })
}

/// What an export writes: container, codecs and whether there is a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Format {
    /// MP4, H.264 + AAC.
    #[default]
    Mp4,
    /// MP4, H.265 + AAC (tagged `hvc1` so Apple players open it).
    Hevc,
    /// QuickTime, ProRes 422 HQ (10-bit) + 24-bit PCM, for editing elsewhere.
    ProRes,
    /// QuickTime, ProRes 4444 with alpha + 24-bit PCM: transparent where no clip draws.
    ProRes4444,
    /// WebM, VP9 + Opus.
    Webm,
    /// Audio only: 24-bit PCM WAV.
    Wav,
    /// Audio only: AAC in M4A.
    M4a,
    /// Audio only: MP3.
    Mp3,
    /// Audio only: FLAC (lossless).
    Flac,
}

impl Format {
    /// Every format with its name (as `render.export` takes it) and a description.
    pub const ALL: [(Format, &'static str, &'static str); 9] = [
        (Format::Mp4, "mp4", "MP4, H.264 + AAC (default; plays everywhere)"),
        (Format::Hevc, "hevc", "MP4, H.265/HEVC + AAC (smaller; needs an HEVC encoder in ffmpeg)"),
        (Format::ProRes, "prores", "QuickTime, ProRes 422 HQ + PCM, for finishing in another editor"),
        (Format::ProRes4444, "prores-4444", "QuickTime, ProRes 4444 with alpha + PCM: transparent where no clip draws (overlays, titles)"),
        (Format::Webm, "webm", "WebM, VP9 + Opus, for the web"),
        (Format::Wav, "wav", "audio only, 24-bit WAV"),
        (Format::M4a, "m4a", "audio only, AAC"),
        (Format::Mp3, "mp3", "audio only, MP3"),
        (Format::Flac, "flac", "audio only, FLAC (lossless)"),
    ];

    pub fn parse(name: &str) -> Option<Format> {
        Self::ALL.iter().find(|(_, n, _)| *n == name).map(|(f, _, _)| *f)
    }

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(f, _, _)| *f == self).map_or("mp4", |(_, n, _)| n)
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Mp4 | Format::Hevc => "mp4",
            Format::ProRes | Format::ProRes4444 => "mov",
            Format::Webm => "webm",
            Format::Wav => "wav",
            Format::M4a => "m4a",
            Format::Mp3 => "mp3",
            Format::Flac => "flac",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Mp4 | Format::Hevc => "video/mp4",
            Format::ProRes | Format::ProRes4444 => "video/quicktime",
            Format::Webm => "video/webm",
            Format::Wav => "audio/wav",
            Format::M4a => "audio/mp4",
            Format::Mp3 => "audio/mpeg",
            Format::Flac => "audio/flac",
        }
    }

    pub fn has_video(self) -> bool {
        !matches!(self, Format::Wav | Format::M4a | Format::Mp3 | Format::Flac)
    }

    /// The picture keeps transparency (only the GPU compositor can draw it).
    pub fn alpha(self) -> bool {
        self == Format::ProRes4444
    }

    /// Pixel format the video is encoded in.
    pub fn pix_fmt(self) -> &'static str {
        match self {
            Format::ProRes => "yuv422p10le",
            Format::ProRes4444 => "yuva444p10le",
            _ => "yuv420p",
        }
    }

    /// The best video encoder the local ffmpeg has for this format.
    pub fn video_encoder(self) -> Result<String> {
        match self {
            Format::Mp4 => Ok(ffmpeg::h264_encoder()?.to_owned()),
            Format::Hevc => ffmpeg::first_encoder(&["libx265", "hevc_videotoolbox", "hevc_nvenc", "hevc_qsv", "hevc_amf", "hevc_mf"]),
            Format::ProRes | Format::ProRes4444 => ffmpeg::first_encoder(&["prores_ks"]),
            Format::Webm => ffmpeg::first_encoder(&["libvpx-vp9"]),
            _ => Err(RenderError::Invalid(format!("{} has no picture", self.name()))),
        }
    }

    /// The ffmpeg audio encoder.
    pub fn audio_codec(self) -> &'static str {
        match self {
            Format::Mp4 | Format::Hevc | Format::M4a => "aac",
            Format::ProRes | Format::ProRes4444 | Format::Wav => "pcm_s24le",
            Format::Webm => "libopus",
            Format::Mp3 => "libmp3lame",
            Format::Flac => "flac",
        }
    }

    /// The audio codec and its options.
    fn audio_args(self, kbps: u32) -> Vec<String> {
        let mut out = strings(&["-c:a", self.audio_codec()]);
        match self {
            Format::Mp4 | Format::Hevc | Format::M4a | Format::Webm => out.extend(strings(&["-b:a", &format!("{kbps}k")])),
            Format::Mp3 => out.extend(strings(&["-b:a", &format!("{}k", kbps.clamp(64, 320))])),
            _ => {}
        }
        out
    }

    /// The container's muxer and its options.
    fn mux_args(self) -> Vec<String> {
        match self {
            Format::Mp4 | Format::Hevc | Format::M4a => strings(&["-movflags", "+faststart", "-f", "mp4"]),
            Format::ProRes | Format::ProRes4444 => strings(&["-f", "mov"]),
            Format::Webm => strings(&["-f", "webm"]),
            Format::Wav => strings(&["-f", "wav"]),
            Format::Mp3 => strings(&["-f", "mp3"]),
            Format::Flac => strings(&["-f", "flac"]),
        }
    }
}

/// Video encoding choices for [`encode`].
#[derive(Debug, Clone)]
pub struct EncodeSettings {
    pub canvas: Canvas,
    /// Container and codecs.
    pub format: Format,
    /// `None` picks the best video encoder this ffmpeg has for `format`.
    pub encoder: Option<String>,
    /// Quality for libx264 (lower is better).
    pub crf: u8,
    /// libx264 speed preset.
    pub x264_preset: String,
    /// Bitrate for encoders without a quality mode, in bits per pixel per frame.
    pub bits_per_pixel: f64,
    pub audio_kbps: u32,
    pub crossfade: Q,
}

pub struct Encoded {
    pub frames: i64,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
}

/// Render `span` to a file at `out` in `s.format` through the ffmpeg graph,
/// with audio effects from `fx`. Audio-only formats skip the picture.
/// Formats with alpha need [`compose::encode`].
pub fn encode(tl: &Timeline, span: Span, s: &EncodeSettings, out: &Path, work: &Path, fx: &fx::Registry) -> Result<Encoded> {
    let p = plan(tl, span)?;
    let c = &s.canvas;
    let f = s.format;
    if f.alpha() {
        return Err(RenderError::Invalid(format!("{} needs the GPU compositor", f.name())));
    }
    let mut g = Graph::new();
    let v = f.has_video().then(|| g.video(&p, c));
    let a = g.audio(&p, c, s.crossfade, fx)?;
    let encoder = match (&s.encoder, f.has_video()) {
        (Some(e), true) => e.clone(),
        (None, true) => f.video_encoder()?,
        (_, false) => f.audio_codec().to_owned(),
    };
    let v = v.map(|v| format!("[{v}]")).unwrap_or_default();
    let args = encode_args(s, &encoder, &v, &format!("[{a}]"), out);
    ffmpeg::run(&g, &args, work, false)?;
    let frames = ((span.end - span.start) * c.rate.as_ratio()).round().to_integer();
    let (width, height) = if f.has_video() { (c.width, c.height) } else { (0, 0) };
    // Audio files have no picture, so warnings about drawing do not apply.
    let warnings = if f.has_video() { p.warnings } else { p.warnings.into_iter().filter(|w| !w.contains(plan::GPU_ONLY)).collect() };
    Ok(Encoded { frames, encoder, width, height, warnings })
}

/// Render `span` to an MP4 at `out` from finished JPEG frames: `frame(t)`
/// gives the frame at each timeline time in order (from a frame cache, or
/// rendered on demand); the audio is the Phase 1 graph as usual. Used for
/// playback previews, whose frames the preview has usually drawn already.
pub fn encode_frames(
    tl: &Timeline,
    span: Span,
    s: &EncodeSettings,
    out: &Path,
    work: &Path,
    fx: &fx::Registry,
    frame: &mut dyn FnMut(Q) -> Result<Vec<u8>>,
) -> Result<Encoded> {
    let p = plan(tl, span)?;
    let c = &s.canvas;
    let r = c.rate.as_ratio();
    let total = ((span.end - span.start) * r).round().to_integer();
    let encoder = match &s.encoder {
        Some(e) => e.clone(),
        None => ffmpeg::h264_encoder()?.to_owned(),
    };
    let mut g = Graph::new();
    let a = g.audio(&p, c, s.crossfade, fx)?;
    let pipe_index = g.input_count();
    g.chain(format!(
        "[{pipe_index}:v]scale={}:{}:out_color_matrix=bt709:out_range=tv,format=yuv420p,setsar=1[gv]",
        c.width, c.height
    ));
    let pipe = ["-f", "image2pipe", "-c:v", "mjpeg", "-framerate", &c.rate.to_string(), "-i", "-"].iter().map(|a| (*a).to_owned()).collect();
    let args = encode_args(s, &encoder, "[gv]", &format!("[{a}]"), out);
    let ctrl = control::current();
    let mut produce = |send: &mut ffmpeg::Send| -> Result<()> {
        for k in 0..total {
            send(frame(span.start + Q::from_integer(k) / r)?)?;
            if let Some(ctrl) = &ctrl {
                ctrl.progress(((k + 1) as f64) / c.rate.to_f64());
            }
        }
        Ok(())
    };
    if let Err(e) = ffmpeg::feed(&ffmpeg::Inputs::GraphPipe(&g, pipe), &args, work, &mut produce) {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    let warnings = p.warnings.into_iter().filter(|w| !w.contains(plan::GPU_ONLY)).collect();
    Ok(Encoded { frames: total, encoder, width: c.width, height: c.height, warnings })
}

/// A hash of everything in a playback preview of `span` at `width`: every
/// frame's [`compose::frame_key`], the audio plan and the encode settings.
/// Equal keys mean an earlier preview file can be played again as is.
pub fn preview_key(tl: &Timeline, span: Span, s: &EncodeSettings, fx: &fx::Registry) -> Result<String> {
    preview_key_from(tl, span, s, &preview_frame_keys(tl, span, s, fx)?, fx)
}

/// The [`compose::frame_key`] of every frame of a preview of `span`, in order.
pub fn preview_frame_keys(tl: &Timeline, span: Span, s: &EncodeSettings, fx: &fx::Registry) -> Result<Vec<String>> {
    let c = &s.canvas;
    let r = c.rate.as_ratio();
    let total = ((span.end - span.start) * r).round().to_integer();
    let first = (span.start * r).round().to_integer();
    let keys = compose::frame_keys(tl, first, first + total, c.width, ImageFormat::Jpeg, fx)?;
    if keys.len() as i64 != total {
        return Err(RenderError::Invalid("the preview range runs past the timeline".into()));
    }
    Ok(keys)
}

/// [`preview_key`] from frame keys already computed ([`preview_frame_keys`]).
pub fn preview_key_from(tl: &Timeline, span: Span, s: &EncodeSettings, frame_keys: &[String], fx: &fx::Registry) -> Result<String> {
    let c = &s.canvas;
    let mut h = blake3::Hasher::new();
    h.update(format!("preview v{} {:?} {} {} {} {} {}", compose::FRAME_KEY_VERSION, c, s.crf, s.x264_preset, s.audio_kbps, s.crossfade, span.start).as_bytes());
    for key in frame_keys {
        h.update(key.as_bytes());
    }
    // Audio: where each piece of sound comes from and how it is placed, relative to the span.
    let p = plan(tl, span)?;
    for t in &p.audio {
        for seg in &t.segments {
            h.update(
                format!(
                    "{} {} {} {} {} {} {:?} {:?} a:{} {}",
                    seg.path.display(),
                    seg.start - span.start,
                    seg.end - span.start,
                    seg.media_start,
                    seg.speed,
                    seg.volume_db,
                    seg.head,
                    seg.tail,
                    seg.stream,
                    seg.channels.label()
                )
                    .as_bytes(),
            );
            // Audio effects: which code (by hash) with which values; times are clip-relative.
            for e in &seg.effects {
                let code = fx.get(&e.effect).map_or(0, |f| f.hash);
                h.update(format!("fx {} {code:x} {} {}", e.effect, seg.clip_start - span.start, serde_json::to_string(e).unwrap_or_default()).as_bytes());
            }
        }
        h.update(b"|");
    }
    Ok(h.finalize().to_hex().to_string())
}

/// ffmpeg output arguments for `s.format` from the video stream `v` and
/// audio stream `a` (stream specifiers, e.g. `[v1]`; `v` is unused for
/// audio-only formats), shared by both backends.
pub(crate) fn encode_args(s: &EncodeSettings, encoder: &str, v: &str, a: &str, out: &Path) -> Vec<String> {
    let c = &s.canvas;
    let f = s.format;
    let mut args = Vec::new();
    if f.has_video() {
        let fps = c.rate.to_f64();
        let bitrate = (f64::from(c.width) * f64::from(c.height) * fps * s.bits_per_pixel) as u64;
        args.extend(strings(&["-map", v, "-c:v", encoder]));
        match encoder {
            "libx264" => args.extend(strings(&["-preset", &s.x264_preset, "-crf", &s.crf.to_string()])),
            // x265's CRF runs about 5 higher than x264's for the same look.
            "libx265" => args.extend(strings(&["-preset", &s.x264_preset, "-crf", &(s.crf + 5).to_string(), "-x265-params", "log-level=error"])),
            "mpeg4" => args.extend(strings(&["-q:v", "3"])),
            // Profile 3 is 422 HQ, 4 is 4444 (with alpha).
            "prores_ks" => {
                args.extend(strings(&["-profile:v", if f.alpha() { "4" } else { "3" }, "-vendor", "apl0"]));
                if f.alpha() {
                    args.extend(strings(&["-alpha_bits", "16"]));
                }
            }
            // VP9 CRF runs about 13 higher than x264's; `-b:v 0` makes it pure quality mode.
            "libvpx-vp9" => args.extend(strings(&["-crf", &(s.crf + 13).to_string(), "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4"])),
            _ => args.extend(strings(&["-b:v", &bitrate.to_string()])),
        }
        if f == Format::Hevc {
            args.extend(strings(&["-tag:v", "hvc1"]));
        }
        args.extend(strings(&["-pix_fmt", f.pix_fmt(), "-r", &c.rate.to_string()]));
    }
    args.extend(strings(&["-map", a]));
    args.extend(f.audio_args(s.audio_kbps));
    args.extend(strings(&["-ar", &c.sample_rate.to_string(), "-ac", "2"]));
    args.extend(f.mux_args());
    args.push(out.to_string_lossy().into_owned());
    args
}

/// Whether rendering `span` needs the GPU compositor (it has transitions or
/// effects, which the ffmpeg graph cannot draw).
pub fn needs_gpu(tl: &Timeline, span: Span) -> Result<bool> {
    Ok(plan(tl, span)?.gpu_features > 0)
}

/// Run the timeline's mixed audio over `span` through `filter` (e.g.
/// `silencedetect=...`) and return ffmpeg's log, where analyzers report.
/// Times in the log are seconds from the span start.
pub fn audio_log(tl: &Timeline, span: Span, filter: &str, work: &Path, fx: &fx::Registry) -> Result<(String, Vec<String>)> {
    let p = plan(tl, span)?;
    let c = project_canvas(tl);
    let mut g = Graph::new();
    let a = g.audio(&p, &c, default_crossfade(), fx)?;
    g.chain(format!("[{a}]{filter}[analysis]"));
    let o = ffmpeg::run(&g, &strings(&["-map", "[analysis]", "-f", "null", "-"]), work, true)?;
    Ok((o.stderr, p.warnings))
}

/// The same for the composited picture, scaled to `width` (analysis needs no more).
pub fn video_log(tl: &Timeline, span: Span, width: u32, filter: &str, work: &Path) -> Result<(String, Vec<String>)> {
    let p = plan(tl, span)?;
    let c = project_canvas(tl).scaled_to_width(width);
    let mut g = Graph::new();
    let v = g.video(&p, &c);
    g.chain(format!("[{v}]{filter}[analysis]"));
    let o = ffmpeg::run(&g, &strings(&["-map", "[analysis]", "-f", "null", "-"]), work, true)?;
    Ok((o.stderr, p.warnings))
}
