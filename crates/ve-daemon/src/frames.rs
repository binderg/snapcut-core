//! The frame cache behind `render.frame`.
//!
//! Frames are stored by [`ve_render::compose::frame_key`]: a hash of what
//! decides their pixels, not of the revision. An edit only misses the frames
//! it changed; a ripple that shifts later frames in time still hits. Two
//! levels: recent frames in memory, and every frame on disk in the project's
//! cache folder (`frames/`), so a restarted daemon starts warm.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Memory budget for encoded frames.
const MEMORY_BYTES: usize = 256 << 20;
/// Disk budget per project; trimmed back to 3/4 of it, oldest files first.
const DISK_BYTES: u64 = 2 << 30;
/// Check the disk budget after this many writes.
const PRUNE_EVERY: u64 = 500;

/// One encoded frame and what `render.frame` reports with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFrame {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(skip)]
    pub bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
struct Memory {
    map: HashMap<String, (CachedFrame, u64)>,
    bytes: usize,
    tick: u64,
}

#[derive(Default)]
pub struct FrameCache {
    memory: Mutex<Memory>,
    writes: AtomicU64,
    /// When `render.frame` last ran for a client (background work waits while someone scrubs).
    last_interactive: Mutex<Option<Instant>>,
}

fn file(dir: &Path, key: &str) -> PathBuf {
    dir.join(&key[..2]).join(format!("{key}.frame"))
}

impl FrameCache {
    /// The frame with this key, from memory or disk.
    pub fn get(&self, dir: &Path, key: &str) -> Option<CachedFrame> {
        {
            let mut m = self.memory.lock().unwrap_or_else(|e| e.into_inner());
            m.tick += 1;
            let tick = m.tick;
            if let Some((f, t)) = m.map.get_mut(key) {
                *t = tick;
                return Some(f.clone());
            }
        }
        let f = read(&file(dir, key))?;
        self.remember(key, f.clone());
        Some(f)
    }

    /// Whether a frame is cached, without loading it.
    pub fn contains(&self, dir: &Path, key: &str) -> bool {
        self.memory.lock().unwrap_or_else(|e| e.into_inner()).map.contains_key(key) || file(dir, key).is_file()
    }

    pub fn put(&self, dir: &Path, key: &str, f: CachedFrame) {
        let path = file(dir, key);
        if let Err(e) = write(&path, &f) {
            tracing::warn!("could not cache frame {}: {e}", path.display());
        } else if self.writes.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY == PRUNE_EVERY - 1 {
            prune(dir);
        }
        self.remember(key, f);
    }

    fn remember(&self, key: &str, f: CachedFrame) {
        let mut m = self.memory.lock().unwrap_or_else(|e| e.into_inner());
        m.tick += 1;
        let tick = m.tick;
        let size = f.bytes.len();
        if let Some((old, _)) = m.map.insert(key.to_owned(), (f, tick)) {
            m.bytes -= old.bytes.len();
        }
        m.bytes += size;
        if m.bytes > MEMORY_BYTES {
            // Drop the least recently used down to 3/4 of the budget.
            let mut order: Vec<(u64, String)> = m.map.iter().map(|(k, (_, t))| (*t, k.clone())).collect();
            order.sort_unstable();
            for (_, k) in order {
                if m.bytes <= MEMORY_BYTES / 4 * 3 {
                    break;
                }
                if let Some((f, _)) = m.map.remove(&k) {
                    m.bytes -= f.bytes.len();
                }
            }
        }
    }

    /// Drop every frame held in memory (after the disk cache was deleted).
    pub fn forget_memory(&self) {
        let mut m = self.memory.lock().unwrap_or_else(|e| e.into_inner());
        m.map.clear();
        m.bytes = 0;
    }

    pub fn touch_interactive(&self) {
        *self.last_interactive.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    /// Milliseconds since a client last asked for a frame (`u64::MAX` if never).
    pub fn idle_ms(&self) -> u64 {
        self.last_interactive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map_or(u64::MAX, |t| t.elapsed().as_millis().try_into().unwrap_or(u64::MAX))
    }
}

/// File layout: one line of JSON (size and warnings), then the image bytes.
fn write(path: &Path, f: &CachedFrame) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("frame files live in a folder"))?;
    let mut out = serde_json::to_vec(f).map_err(std::io::Error::other)?;
    out.push(b'\n');
    out.extend_from_slice(&f.bytes);
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, out)?;
    std::fs::rename(tmp, path)
}

fn read(path: &Path) -> Option<CachedFrame> {
    let data = std::fs::read(path).ok()?;
    let nl = data.iter().position(|b| *b == b'\n')?;
    let mut f: CachedFrame = serde_json::from_slice(&data[..nl]).ok()?;
    f.bytes = Arc::new(data[nl + 1..].to_vec());
    Some(f)
}

/// Keep the folder under its budget, oldest files first.
fn prune(dir: &Path) {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for sub in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        for e in std::fs::read_dir(sub.path()).into_iter().flatten().flatten() {
            if let Ok(m) = e.metadata() {
                files.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()));
            }
        }
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= DISK_BYTES {
        return;
    }
    files.sort();
    for (_, len, path) in files {
        if total <= DISK_BYTES / 4 * 3 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_in_memory_and_on_disk() {
        let dir = std::env::temp_dir().join(format!("ve-frames-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = FrameCache::default();
        let key = "ab12cd";
        assert!(c.get(&dir, key).is_none());
        c.put(&dir, key, CachedFrame { width: 4, height: 2, warnings: vec!["w".into()], bytes: Arc::new(vec![1, 2, 3, b'\n', 5]) });
        assert!(c.contains(&dir, key));
        // A fresh cache (a restarted daemon) reads it back from disk.
        let fresh = FrameCache::default();
        let f = fresh.get(&dir, key).unwrap();
        assert_eq!((f.width, f.height, f.warnings.clone(), f.bytes.as_slice()), (4, 2, vec!["w".to_owned()], &[1u8, 2, 3, b'\n', 5][..]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
