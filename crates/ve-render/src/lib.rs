//! Rendering.
//!
//! 1. Resolver: for time `t` (or a range), the active clips per track with their
//!    source times after speed and trims, evaluated keyframes, transitions.
//! 2. Phase 1 backend: compile the resolved range into one FFmpeg
//!    `-filter_complex` graph (trim/atrim, setpts, scale/crop, concat, afade).
//!    `render.frame` = same graph + `-ss` + `-frames:v 1`.
//! 3. Phase 2 backend: wgpu compositor running WGSL effect plugins; export pipes
//!    composited frames into the FFmpeg encoder so preview and export share one graph.
//!
//! Determinism rule: a frame is a pure function of (project, t). That is what
//! makes the frame cache safe.
