//! Built-in analyzers, implemented with FFmpeg filters: `silencedetect`,
//! `scdet` (scene cuts), `ebur128` (loudness). Output is markers + analysis
//! assets. Moves to `plugins/core` once the process plugin host exists
//! (dogfooding rule: built-ins use the same API as third-party plugins).
