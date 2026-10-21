//! Running the `ffmpeg` binary: version and encoder detection, and one
//! function that runs a compiled graph.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::RenderError;
use crate::graph::Graph;

pub fn ffmpeg_path() -> PathBuf {
    ffmpeg_sidecar::paths::ffmpeg_path()
}

/// Major version of the ffmpeg binary, if it says. Git builds (`N-1234-g...`) give `None`.
pub fn major_version() -> Option<u32> {
    static V: OnceLock<Option<u32>> = OnceLock::new();
    *V.get_or_init(|| {
        let out = Command::new(ffmpeg_path()).arg("-version").output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let word = text.split_whitespace().skip_while(|w| *w != "version").nth(1)?;
        let digits: String = word.trim_start_matches('n').chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// An encoder the local ffmpeg can actually use (listed and able to open).
pub fn h264_encoder() -> Result<&'static str, RenderError> {
    static E: OnceLock<Option<&'static str>> = OnceLock::new();
    // libx264 when the user's ffmpeg has it (their build, their license);
    // then hardware encoders, then openh264, then MPEG-4 Part 2 so an
    // LGPL-only build still produces a playable MP4.
    const CANDIDATES: [&str; 8] =
        ["libx264", "h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf", "h264_mf", "libopenh264", "mpeg4"];
    E.get_or_init(|| CANDIDATES.into_iter().find(|e| encoder_works(e)))
        .ok_or_else(|| RenderError::Ffmpeg { message: "no usable video encoder in this ffmpeg".into(), stderr: String::new() })
}

fn encoder_works(name: &str) -> bool {
    Command::new(ffmpeg_path())
        .args(["-hide_banner", "-nostdin", "-v", "error", "-f", "lavfi", "-i", "color=s=256x256:r=25:d=0.2", "-frames:v", "3", "-c:v", name, "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// What a run produced.
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Run `graph` with output arguments `out` (maps, codecs, destination).
/// `log_info` keeps filter log lines (analyzers read them from stderr).
pub fn run(graph: &Graph, out: &[String], work: &Path, log_info: bool) -> Result<Output, RenderError> {
    std::fs::create_dir_all(work).map_err(|e| RenderError::Io(format!("{}: {e}", work.display())))?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let script = work.join(format!("graph-{}-{nanos}.txt", std::process::id()));
    std::fs::write(&script, graph.script()).map_err(|e| RenderError::Io(format!("{}: {e}", script.display())))?;
    // Long graphs go through a file: Windows caps command lines at 32K chars.
    let script_flag = match major_version() {
        Some(v) if v < 7 => "-filter_complex_script",
        _ => "-/filter_complex",
    };
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-y", "-loglevel", if log_info { "info" } else { "error" }]);
    cmd.args(graph.input_args());
    cmd.arg(script_flag).arg(&script);
    cmd.args(out);
    tracing::debug!(?cmd, "ffmpeg");
    let result = cmd.stdin(Stdio::null()).output();
    // VE_KEEP_GRAPH=1 keeps the script next to the renders for debugging.
    if std::env::var_os("VE_KEEP_GRAPH").is_none() {
        let _ = std::fs::remove_file(&script);
    } else {
        tracing::info!("kept filter graph {}", script.display());
    }
    let output = result.map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => RenderError::Ffmpeg {
            message: format!("ffmpeg not found (tried {}); install FFmpeg and put it on PATH", ffmpeg_path().display()),
            stderr: String::new(),
        },
        _ => RenderError::Io(e.to_string()),
    })?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let tail: Vec<&str> = stderr.lines().rev().filter(|l| !l.trim().is_empty()).take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(RenderError::Ffmpeg { message: "ffmpeg failed".into(), stderr: tail.join("\n") });
    }
    Ok(Output { stdout: output.stdout, stderr })
}
