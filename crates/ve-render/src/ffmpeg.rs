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

/// Where a run reads from: a compiled timeline graph, or plain input
/// arguments (`-i file ...`) for working on one media file directly.
pub enum Inputs<'a> {
    Graph(&'a Graph),
    Args(Vec<String>),
}

/// Run `graph` with output arguments `out` (maps, codecs, destination).
/// `log_info` keeps filter log lines (analyzers read them from stderr).
pub fn run(graph: &Graph, out: &[String], work: &Path, log_info: bool) -> Result<Output, RenderError> {
    run_inputs(&Inputs::Graph(graph), out, work, log_info)
}

/// [`run`] for any [`Inputs`].
pub fn run_inputs(inputs: &Inputs<'_>, out: &[String], work: &Path, log_info: bool) -> Result<Output, RenderError> {
    let (mut cmd, script) = command(inputs, out, work, log_info)?;
    let result = cmd.stdin(Stdio::null()).output();
    drop(script);
    let output = result.map_err(spawn_error)?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    check(output.status, &stderr)?;
    Ok(Output { stdout: output.stdout, stderr })
}

/// Run and hand stdout to `sink` as it arrives, for outputs too big to hold
/// (decoded samples of a long file). Returns the log.
pub fn stream(inputs: &Inputs<'_>, out: &[String], work: &Path, sink: &mut dyn FnMut(&[u8])) -> Result<String, RenderError> {
    let (mut cmd, script) = command(inputs, out, work, false)?;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(spawn_error)?;
    // Drain stderr on its own thread so a chatty ffmpeg never blocks on a full pipe.
    let mut err_pipe = child.stderr.take().expect("stderr is piped");
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err_pipe, &mut s);
        s
    });
    let mut out_pipe = child.stdout.take().expect("stdout is piped");
    let mut buf = vec![0u8; 1 << 16];
    loop {
        match std::io::Read::read(&mut out_pipe, &mut buf) {
            Ok(0) => break,
            Ok(n) => sink(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(RenderError::Io(format!("reading ffmpeg output: {e}"))),
        }
    }
    let status = child.wait().map_err(|e| RenderError::Io(e.to_string()))?;
    let stderr = err_thread.join().unwrap_or_default();
    drop(script);
    check(status, &stderr)?;
    Ok(stderr)
}

/// A filter script file, removed when dropped (unless `VE_KEEP_GRAPH` is set).
struct Script(Option<PathBuf>);

impl Drop for Script {
    fn drop(&mut self) {
        let Some(path) = self.0.take() else { return };
        // VE_KEEP_GRAPH=1 keeps the script next to the renders for debugging.
        if std::env::var_os("VE_KEEP_GRAPH").is_none() {
            let _ = std::fs::remove_file(&path);
        } else {
            tracing::info!("kept filter graph {}", path.display());
        }
    }
}

fn command(inputs: &Inputs<'_>, out: &[String], work: &Path, log_info: bool) -> Result<(Command, Script), RenderError> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-y", "-loglevel", if log_info { "info" } else { "error" }]);
    let script = match inputs {
        Inputs::Graph(graph) => {
            std::fs::create_dir_all(work).map_err(|e| RenderError::Io(format!("{}: {e}", work.display())))?;
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let script = work.join(format!("graph-{}-{nanos}.txt", std::process::id()));
            std::fs::write(&script, graph.script()).map_err(|e| RenderError::Io(format!("{}: {e}", script.display())))?;
            // Long graphs go through a file: Windows caps command lines at 32K chars.
            let script_flag = match major_version() {
                Some(v) if v < 7 => "-filter_complex_script",
                _ => "-/filter_complex",
            };
            cmd.args(graph.input_args());
            cmd.arg(script_flag).arg(&script);
            Script(Some(script))
        }
        Inputs::Args(args) => {
            cmd.args(args);
            Script(None)
        }
    };
    cmd.args(out);
    tracing::debug!(?cmd, "ffmpeg");
    Ok((cmd, script))
}

fn spawn_error(e: io::Error) -> RenderError {
    match e.kind() {
        io::ErrorKind::NotFound => RenderError::Ffmpeg {
            message: format!("ffmpeg not found (tried {}); install FFmpeg and put it on PATH", ffmpeg_path().display()),
            stderr: String::new(),
        },
        _ => RenderError::Io(e.to_string()),
    }
}

fn check(status: std::process::ExitStatus, stderr: &str) -> Result<(), RenderError> {
    if status.success() {
        return Ok(());
    }
    let tail: Vec<&str> = stderr.lines().rev().filter(|l| !l.trim().is_empty()).take(12).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    Err(RenderError::Ffmpeg { message: "ffmpeg failed".into(), stderr: tail.join("
") })
}
