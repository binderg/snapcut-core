//! Running the `ffmpeg` binary: version and encoder detection, and one
//! function that runs a compiled graph.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::RenderError;
use crate::control::{self, Control};
use crate::graph::Graph;

pub fn ffmpeg_path() -> PathBuf {
    ve_media::ffmpeg_path()
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

/// The first of `names` the local ffmpeg can use (checked once per name).
pub fn first_encoder(names: &[&str]) -> Result<String, RenderError> {
    static SEEN: OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> = OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    for name in names {
        let cached = seen.lock().ok().and_then(|m| m.get(*name).copied());
        let works = cached.unwrap_or_else(|| {
            let w = encoder_works(name);
            if let Ok(mut m) = seen.lock() {
                m.insert((*name).to_owned(), w);
            }
            w
        });
        if works {
            return Ok((*name).to_owned());
        }
    }
    Err(RenderError::Ffmpeg { message: format!("this ffmpeg has none of these encoders: {}", names.join(", ")), stderr: String::new() })
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
    /// A graph plus one more input after its files (raw frames on stdin for [`feed`]).
    GraphPipe(&'a Graph, Vec<String>),
    Args(Vec<String>),
}

/// Run `graph` with output arguments `out` (maps, codecs, destination).
/// `log_info` keeps filter log lines (analyzers read them from stderr).
pub fn run(graph: &Graph, out: &[String], work: &Path, log_info: bool) -> Result<Output, RenderError> {
    run_inputs(&Inputs::Graph(graph), out, work, log_info)
}

/// [`run`] for any [`Inputs`].
pub fn run_inputs(inputs: &Inputs<'_>, out: &[String], work: &Path, log_info: bool) -> Result<Output, RenderError> {
    let mut stdout = Vec::new();
    let stderr = exec(inputs, out, work, log_info, &mut |b| stdout.extend_from_slice(b))?;
    Ok(Output { stdout, stderr })
}

/// Run and hand stdout to `sink` as it arrives, for outputs too big to hold
/// (decoded samples of a long file). Returns the log.
pub fn stream(inputs: &Inputs<'_>, out: &[String], work: &Path, sink: &mut dyn FnMut(&[u8])) -> Result<String, RenderError> {
    exec(inputs, out, work, false, sink)
}

/// Spawn ffmpeg, feed its stdout to `sink` on this thread and return its log.
/// With a [`Control`] installed for this thread, a watcher thread reports
/// progress from ffmpeg's `-progress` file and kills the run when the job is
/// cancelled.
fn exec(inputs: &Inputs<'_>, out: &[String], work: &Path, log_info: bool, sink: &mut dyn FnMut(&[u8])) -> Result<String, RenderError> {
    let ctrl = control::current();
    if ctrl.as_ref().is_some_and(|c| c.cancelled()) {
        return Err(RenderError::Cancelled);
    }
    let progress = match &ctrl {
        Some(_) => Some(TempFile::new(work, "progress")?),
        None => None,
    };
    let (mut cmd, script) = command(inputs, out, work, log_info, progress.as_ref().map(|p| p.0.as_path()))?;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(spawn_error)?;
    // Drain stderr on its own thread so a chatty ffmpeg never blocks on a full pipe.
    let mut err_pipe = child.stderr.take().expect("stderr is piped");
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err_pipe, &mut s);
        s
    });
    let mut out_pipe = child.stdout.take().expect("stdout is piped");
    let child = Arc::new(Mutex::new(child));
    let done = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let watcher = match (&ctrl, &progress) {
        (Some(ctrl), Some(file)) => {
            let (ctrl, child, done, cancelled, file) = (ctrl.clone(), child.clone(), done.clone(), cancelled.clone(), file.0.clone());
            Some(std::thread::spawn(move || watch(&ctrl, &child, &done, &cancelled, &file)))
        }
        _ => None,
    };
    let mut buf = vec![0u8; 1 << 16];
    let read = loop {
        match std::io::Read::read(&mut out_pipe, &mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => sink(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => break Err(RenderError::Io(format!("reading ffmpeg output: {e}"))),
        }
    };
    if read.is_err() {
        kill_tree(&mut lock(&child));
    }
    // Poll rather than block in `wait()` holding the lock, so the watcher can
    // still kill the run (ffmpeg may close stdout long before it exits).
    let status = loop {
        match lock(&child).try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) => {}
            Err(e) => break Err(RenderError::Io(e.to_string())),
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    done.store(true, Ordering::SeqCst);
    if let Some(w) = watcher {
        let _ = w.join();
    }
    let stderr = err_thread.join().unwrap_or_default();
    drop(script);
    drop(progress);
    if cancelled.load(Ordering::SeqCst) {
        return Err(RenderError::Cancelled);
    }
    read?;
    check(status?, &stderr)?;
    Ok(stderr)
}

/// What [`feed`] hands its producer: queue one chunk for ffmpeg's stdin.
pub type Send<'a> = dyn FnMut(Vec<u8>) -> Result<(), RenderError> + 'a;

/// Frames waiting for ffmpeg's stdin; bounds memory and lets the compositor
/// run ahead of the encoder by a few frames.
const FEED_QUEUE: usize = 4;

/// Run ffmpeg with `produce` filling its stdin: it calls the given function
/// with each chunk (a raw frame) in order. Stops with [`RenderError::Cancelled`]
/// when this thread's job is cancelled (checked by `produce` through
/// [`control::cancelled`], and here between frames). Returns ffmpeg's log.
pub fn feed(
    inputs: &Inputs<'_>,
    out: &[String],
    work: &Path,
    produce: &mut dyn FnMut(&mut Send) -> Result<(), RenderError>,
) -> Result<String, RenderError> {
    if control::cancelled() {
        return Err(RenderError::Cancelled);
    }
    let (mut cmd, script) = command(inputs, out, work, false, None)?;
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(spawn_error)?;
    let mut err_pipe = child.stderr.take().expect("stderr is piped");
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err_pipe, &mut s);
        s
    });
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(FEED_QUEUE);
    let writer = std::thread::spawn(move || {
        for chunk in rx {
            if std::io::Write::write_all(&mut stdin, &chunk).is_err() {
                return false;
            }
        }
        true
    });
    // Set when ffmpeg stopped taking frames: then its own log says why.
    let ffmpeg_gone = std::cell::Cell::new(false);
    let mut send = |chunk: Vec<u8>| -> Result<(), RenderError> {
        if control::cancelled() {
            return Err(RenderError::Cancelled);
        }
        tx.send(chunk).map_err(|_| {
            ffmpeg_gone.set(true);
            RenderError::Io("ffmpeg stopped reading frames".into())
        })
    };
    let produced = produce(&mut send);
    drop(tx);
    let child = Mutex::new(child);
    // The producer failed (cancel, a frame that would not render): stop
    // ffmpeg now rather than let it finish a short file.
    let producer_failed = produced.is_err() && !ffmpeg_gone.get();
    if producer_failed {
        kill_tree(&mut lock(&child));
    }
    let wrote = writer.join().unwrap_or(false);
    let status = loop {
        match lock(&child).try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) => {}
            Err(e) => break Err(RenderError::Io(e.to_string())),
        }
        if control::cancelled() {
            kill_tree(&mut lock(&child));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stderr = err_thread.join().unwrap_or_default();
    drop(script);
    if matches!(produced, Err(RenderError::Cancelled)) || control::cancelled() {
        return Err(RenderError::Cancelled);
    }
    if producer_failed {
        return produced.map(|()| String::new());
    }
    let status = status?;
    // ffmpeg's own error is the useful one when the pipe broke.
    if !status.success() {
        check(status, &stderr)?;
    }
    produced?;
    if !wrote {
        return Err(RenderError::Io("ffmpeg closed its input early".into()));
    }
    Ok(stderr)
}

fn lock(child: &Mutex<Child>) -> std::sync::MutexGuard<'_, Child> {
    child.lock().unwrap_or_else(|e| e.into_inner())
}

/// Stop a run and anything it started. On Windows `ffmpeg` is often a
/// package manager's shim (Chocolatey, Scoop) that runs the real ffmpeg as a
/// child: killing only the shim leaves the encode running and the pipes open.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
}

/// Report progress and honour cancel until the run is done.
fn watch(ctrl: &Control, child: &Mutex<Child>, done: &AtomicBool, cancelled: &AtomicBool, progress: &Path) {
    let mut offset = 0u64;
    let mut last = None;
    while !done.load(Ordering::SeqCst) {
        if ctrl.cancelled() {
            cancelled.store(true, Ordering::SeqCst);
            kill_tree(&mut lock(child));
            return;
        }
        if let Some(secs) = read_progress(progress, &mut offset)
            && last != Some(secs)
        {
            last = Some(secs);
            ctrl.progress(secs);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The newest `out_time_us` in ffmpeg's progress file, reading on from `offset`.
fn read_progress(path: &Path, offset: &mut u64) -> Option<f64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(*offset)).ok()?;
    let mut text = String::new();
    f.read_to_string(&mut text).ok()?;
    // Only whole lines; a half-written one is read next time.
    let complete = text.rfind('\n').map_or(0, |i| i + 1);
    *offset += complete as u64;
    parse_out_time(&text[..complete])
}

/// Seconds from the last `out_time_us=` (or the older, misnamed `out_time_ms=`, also microseconds).
fn parse_out_time(text: &str) -> Option<f64> {
    text.lines().rev().find_map(|l| {
        let v = l.strip_prefix("out_time_us=").or_else(|| l.strip_prefix("out_time_ms="))?;
        v.trim().parse::<i64>().ok().filter(|us| *us >= 0).map(|us| us as f64 / 1e6)
    })
}

/// A file in the work folder, removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(work: &Path, kind: &str) -> Result<Self, RenderError> {
        std::fs::create_dir_all(work).map_err(|e| RenderError::Io(format!("{}: {e}", work.display())))?;
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Ok(Self(work.join(format!("{kind}-{}-{nanos}.txt", std::process::id()))))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
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

fn command(
    inputs: &Inputs<'_>,
    out: &[String],
    work: &Path,
    log_info: bool,
    progress: Option<&Path>,
) -> Result<(Command, Script), RenderError> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-y", "-loglevel", if log_info { "info" } else { "error" }]);
    if let Some(p) = progress {
        cmd.arg("-progress").arg(p);
    }
    let script = match inputs {
        Inputs::Graph(graph) | Inputs::GraphPipe(graph, _) => {
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
            if let Inputs::GraphPipe(_, pipe) = inputs {
                cmd.args(pipe);
            }
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
            message: format!("ffmpeg not found (tried {}); put ffmpeg next to ve or on PATH, or set VE_FFMPEG", ffmpeg_path().display()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_newest_out_time() {
        let text = "frame=10\nout_time_us=400000\nprogress=continue\nframe=20\nout_time_ms=900000\nprogress=continue\n";
        assert_eq!(parse_out_time(text), Some(0.9));
        assert_eq!(parse_out_time("out_time_us=N/A\n"), None);
        assert_eq!(parse_out_time(""), None);
    }
}
