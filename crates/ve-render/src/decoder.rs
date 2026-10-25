//! Warm decoders: long-lived ffmpeg processes that stream raw RGBA frames.
//!
//! Starting ffmpeg costs about 100 ms on Windows; decoding one more 1080p
//! frame from an open stream costs a few ms. So each layer keeps a process
//! open, positioned after the last frame it returned:
//!
//! - the next frame, or a short jump ahead, is read from the open pipe;
//! - a jump back, or further ahead than [`SKIP_LIMIT`] frames, restarts that
//!   process with a seek;
//! - processes idle longer than [`IDLE`] are closed.
//!
//! ffmpeg conforms to the source's nominal rate (`fps`, as the Phase 1 graph
//! does for VFR footage) and scales to the size the frame is drawn at, with
//! the same default scaler as the Phase 1 graph.
//!
//! Later: in-process decoders (`ffmpeg-the-third`) make random seeks cheap
//! too. This module's [`DecoderPool::frame`] is the seam to swap.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ve_model::time::{Q, Rate};

use crate::RenderError;
use crate::ffmpeg::ffmpeg_path;

type Result<T> = std::result::Result<T, RenderError>;

/// Read through at most this many frames to reach a later one; further
/// jumps restart the process (cheaper than decoding that many frames).
pub const SKIP_LIMIT: i64 = 30;

/// Close a decoder unused this long.
pub const IDLE: Duration = Duration::from_secs(30);

/// Most decoders kept open at once; the least recently used goes first.
pub const MAX_OPEN: usize = 16;

/// What a decoder produces. Two layers showing the same file at the same
/// size still get separate decoders (`slot`), so they do not fight over one
/// stream position.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    /// Which layer (track) asks; keeps one stream per layer.
    pub slot: usize,
    /// A still image: decoded once (with its transparency) and kept, not streamed.
    pub still: bool,
}

struct Decoder {
    child: Child,
    stdout: ChildStdout,
    /// Source frame the next read returns.
    next: i64,
    last_used: Instant,
    stderr: Arc<Mutex<String>>,
}

impl Decoder {
    fn start(key: &Key, from: i64) -> Result<Self> {
        let r = key.rate.as_ratio();
        // Seek half a frame early: the first frame at or after that point is
        // `from`, whatever rounding the container's timestamps have.
        let at = (Q::from_integer(from) - Q::new(1, 2)) / r;
        let at = if at < Q::from_integer(0) { 0.0 } else { *at.numer() as f64 / *at.denom() as f64 };
        let mut cmd = Command::new(ffmpeg_path());
        cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error"])
            .arg("-ss")
            .arg(format!("{at:.6}"))
            .arg("-i")
            .arg(&key.path)
            .args(["-map", "0:v:0", "-an", "-sn", "-vf"])
            // Scale in 4:2:0 YUV, then convert, as the Phase 1 graph does
            // (`scale`, `format=yuv420p`): scaling straight to RGBA places
            // chroma differently at color edges.
            .arg(format!("fps={},scale={}:{},format=yuv420p", key.rate, key.width, key.height))
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        tracing::debug!(?cmd, "decoder start");
        let mut child = cmd.spawn().map_err(|e| RenderError::Io(format!("starting ffmpeg: {e}")))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut err = child.stderr.take().expect("stderr is piped");
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        // Drain stderr so a chatty ffmpeg never blocks; keep the tail for errors.
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = err.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if let Ok(mut s) = sink.lock() {
                    s.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if s.len() > 8192 {
                        let cut = s.len() - 4096;
                        s.drain(..cut);
                    }
                }
            }
        });
        Ok(Self { child, stdout, next: from, last_used: Instant::now(), stderr })
    }

    /// Read one frame into `buf`. `Ok(false)` at the end of the stream.
    fn read(&mut self, buf: &mut [u8]) -> Result<bool> {
        let mut filled = 0;
        while filled < buf.len() {
            match self.stdout.read(&mut buf[filled..]) {
                Ok(0) => return if filled == 0 { Ok(false) } else { Err(self.error("stream ended inside a frame")) },
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(self.error(&e.to_string())),
            }
        }
        self.next += 1;
        Ok(true)
    }

    fn error(&self, what: &str) -> RenderError {
        let log = self.stderr.lock().map(|s| s.trim().to_owned()).unwrap_or_default();
        RenderError::Ffmpeg { message: format!("decoder: {what}"), stderr: log }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Most decoded still images kept per pool; the least recently used goes first.
pub const MAX_STILLS: usize = 32;

/// Decode a still image once at `width x height`, as straight-alpha RGBA.
/// Scaled in 4:2:0 YUV with alpha, as the Phase 1 graph does for stills.
fn decode_still(key: &Key) -> Result<Vec<u8>> {
    let out = Command::new(ffmpeg_path())
        .args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-i"])
        .arg(&key.path)
        .args(["-map", "0:v:0", "-frames:v", "1", "-vf"])
        .arg(format!("scale={}:{},format=yuva420p", key.width, key.height))
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| RenderError::Io(format!("starting ffmpeg: {e}")))?;
    let size = (key.width as usize) * (key.height as usize) * 4;
    if !out.status.success() || out.stdout.len() < size {
        return Err(RenderError::Ffmpeg {
            message: format!("decoder: could not read the image {}", key.path.display()),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    let mut rgba = out.stdout;
    rgba.truncate(size);
    Ok(rgba)
}

/// A decoded still: its file and the size it was decoded at.
type StillKey = (PathBuf, u32, u32);

/// Open decoders, by what they produce.
#[derive(Default)]
pub struct DecoderPool {
    open: HashMap<Key, Decoder>,
    /// Decoded still images by path and size (the slot does not matter), with last use.
    stills: HashMap<StillKey, (Arc<Vec<u8>>, Instant)>,
    /// Processes started (for tests and stats).
    pub starts: u64,
}

impl DecoderPool {
    /// The process-wide pool.
    pub fn shared() -> &'static Mutex<DecoderPool> {
        static POOL: std::sync::OnceLock<Mutex<DecoderPool>> = std::sync::OnceLock::new();
        POOL.get_or_init(|| Mutex::new(DecoderPool::default()))
    }

    /// Source frame `n` of `key.path`, conformed to `key.rate` and scaled to
    /// `key.width x key.height`, as RGBA. Past the end of the file, the last
    /// frame.
    pub fn frame(&mut self, key: &Key, n: i64) -> Result<Vec<u8>> {
        if key.still {
            return Ok(self.still(key)?.as_ref().clone());
        }
        self.close_idle();
        let n = n.max(0);
        let size = (key.width as usize) * (key.height as usize) * 4;
        let mut buf = vec![0u8; size];

        let reusable = self.open.get(key).is_some_and(|d| d.next <= n && n - d.next <= SKIP_LIMIT);
        if !reusable {
            self.open.remove(key);
            self.make_room();
            self.open.insert(key.clone(), Decoder::start(key, n)?);
            self.starts += 1;
        }
        let d = self.open.get_mut(key).expect("just ensured");
        d.last_used = Instant::now();
        let mut got = false;
        while d.next <= n {
            if !d.read(&mut buf)? {
                break;
            }
            got = true;
        }
        if got {
            return Ok(buf);
        }
        // Nothing at `n`: the file ends earlier. Show its last frame, as the
        // Phase 1 graph holds the last frame.
        let msg = d.error(&format!("no frame {n} in {}", key.path.display()));
        self.open.remove(key);
        if n > 0 { self.last_frame(key, n) } else { Err(msg) }
    }

    /// A still image at the key's size, decoded on first use.
    fn still(&mut self, key: &Key) -> Result<Arc<Vec<u8>>> {
        let k = (key.path.clone(), key.width, key.height);
        if let Some((rgba, used)) = self.stills.get_mut(&k) {
            *used = Instant::now();
            return Ok(Arc::clone(rgba));
        }
        let rgba = Arc::new(decode_still(key)?);
        self.starts += 1;
        if self.stills.len() >= MAX_STILLS
            && let Some(oldest) = self.stills.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| k.clone())
        {
            self.stills.remove(&oldest);
        }
        self.stills.insert(k, (Arc::clone(&rgba), Instant::now()));
        Ok(rgba)
    }

    /// Step back from `n` until a frame decodes (only for a request past the end).
    fn last_frame(&mut self, key: &Key, n: i64) -> Result<Vec<u8>> {
        let mut back = 1;
        while back <= n {
            let mut d = Decoder::start(key, n - back)?;
            self.starts += 1;
            let mut buf = vec![0u8; (key.width as usize) * (key.height as usize) * 4];
            let mut got = false;
            while d.read(&mut buf)? {
                got = true;
                if d.next > n {
                    break;
                }
            }
            if got {
                return Ok(buf);
            }
            back *= 4;
        }
        Err(RenderError::Media(format!("{}: no video frames could be decoded", key.path.display())))
    }

    fn close_idle(&mut self) {
        self.open.retain(|_, d| d.last_used.elapsed() < IDLE);
    }

    fn make_room(&mut self) {
        while self.open.len() >= MAX_OPEN {
            let oldest = self.open.iter().min_by_key(|(_, d)| d.last_used).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    self.open.remove(&k);
                }
                None => break,
            }
        }
    }

    /// Close every decoder (tests, or when the project changes media).
    pub fn clear(&mut self) {
        self.open.clear();
        self.stills.clear();
    }

    pub fn open_count(&self) -> usize {
        self.open.len()
    }
}

/// The shared pool, locked.
pub fn shared() -> Result<std::sync::MutexGuard<'static, DecoderPool>> {
    DecoderPool::shared().lock().map_err(|_| RenderError::Io("decoder pool poisoned".into()))
}

/// Keep `Path` in the public signature family for callers building keys.
pub fn key(path: &Path, width: u32, height: u32, rate: Rate, slot: usize) -> Key {
    Key { path: path.to_path_buf(), width, height, rate, slot, still: false }
}
