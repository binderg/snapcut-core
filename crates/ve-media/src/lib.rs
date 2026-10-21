//! Media I/O.
//!
//! Phase 1 drives the `ffmpeg` and `ffprobe` binaries through ffmpeg-sidecar:
//! `media.probe` is `ffprobe -print_format json -show_streams -show_format`
//! deserialized into our own structs. Phase 2 adds an in-process decoder pool
//! (ffmpeg-the-third) for GOP-aware seeks into the compositor.
//!
//! Media is addressed by URI (`file://` now, `s3://` later), never by bare path.

/// True when an `ffmpeg` binary is reachable (PATH, or next to the engine).
pub fn ffmpeg_available() -> bool {
    ffmpeg_sidecar::command::ffmpeg_is_installed()
}

/// Content hash used in the asset table.
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
