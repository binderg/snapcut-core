//! Audio to measure: the timeline's mix over a span (what plays, after cuts,
//! speed and volume), or one media file as it is on disk.

use std::path::Path;

use ve_model::Timeline;

use crate::ffmpeg::{self, Inputs};
use crate::graph::Graph;
use crate::plan::{Span, plan};
use crate::{RenderError, default_crossfade, project_canvas};

type Result<T> = std::result::Result<T, RenderError>;

pub enum AudioSource<'a> {
    /// The mixed timeline audio over `span`, with audio effects from `fx`.
    Timeline { tl: &'a Timeline, span: Span, fx: &'a crate::fx::Registry },
    /// One audio stream of a media file (`a:N`), whole, channels as recorded.
    File { path: &'a Path, stream: u32 },
}

/// The ffmpeg inputs and the label or map that yields the source's audio.
/// `filter` (if any) is applied before the output. Returns the plan's warnings.
fn inputs(src: &AudioSource<'_>, filter: Option<&str>) -> Result<(Inputs<'static>, Vec<String>, Option<Graph>)> {
    match src {
        AudioSource::Timeline { tl, span, fx } => {
            let p = plan(tl, *span)?;
            let c = project_canvas(tl);
            let mut g = Graph::new();
            let a = g.audio(&p, &c, default_crossfade(), fx)?;
            g.chain(format!("[{a}]{}[measure]", filter.unwrap_or("anull")));
            Ok((Inputs::Args(Vec::new()), p.warnings, Some(g)))
        }
        AudioSource::File { path, stream } => {
            if !path.is_file() {
                return Err(RenderError::Media(format!("{} is missing", path.display())));
            }
            let mut args = vec!["-i".to_owned(), path.to_string_lossy().into_owned(), "-map".into(), format!("0:a:{stream}")];
            if let Some(f) = filter {
                args.extend(["-af".into(), f.to_owned()]);
            }
            Ok((Inputs::Args(args), Vec::new(), None))
        }
    }
}

fn out_args(graph: bool, tail: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if graph {
        out.extend(["-map".into(), "[measure]".into()]);
    }
    out.extend(tail.iter().map(|s| (*s).to_owned()));
    out
}

/// Run `filter` (e.g. `ebur128=peak=true`) over the source's audio and
/// return ffmpeg's log, where the filter reports, plus the plan's warnings.
pub fn audio_filter_log(src: &AudioSource<'_>, filter: &str, work: &Path) -> Result<(String, Vec<String>)> {
    let (args, warnings, graph) = inputs(src, Some(filter))?;
    let out = out_args(graph.is_some(), &["-f", "null", "-"]);
    let o = match &graph {
        Some(g) => ffmpeg::run(g, &out, work, true)?,
        None => ffmpeg::run_inputs(&args, &out, work, true)?,
    };
    Ok((o.stderr, warnings))
}

/// Decode the source's audio as interleaved 32-bit float at `sample_rate`
/// with `channels` channels, handing it to `sink` in chunks of whole frames
/// (one sample per channel). Returns the plan's warnings.
pub fn decode_audio(src: &AudioSource<'_>, sample_rate: u32, channels: u32, work: &Path, sink: &mut dyn FnMut(&[f32])) -> Result<Vec<String>> {
    let (args, warnings, graph) = inputs(src, None)?;
    let (sr, ch) = (sample_rate.to_string(), channels.to_string());
    let out = out_args(graph.is_some(), &["-ac", &ch, "-ar", &sr, "-f", "f32le", "-"]);
    let frame_bytes = 4 * channels.max(1) as usize;
    let mut carry: Vec<u8> = Vec::new();
    let mut samples: Vec<f32> = Vec::new();
    let mut on_bytes = |bytes: &[u8]| {
        carry.extend_from_slice(bytes);
        let whole = carry.len() / frame_bytes * frame_bytes;
        samples.clear();
        samples.extend(carry[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
        carry.drain(..whole);
        if !samples.is_empty() {
            sink(&samples);
        }
    };
    let inputs = match &graph {
        Some(g) => Inputs::Graph(g),
        None => args,
    };
    ffmpeg::stream(&inputs, &out, work, &mut on_bytes)?;
    Ok(warnings)
}
