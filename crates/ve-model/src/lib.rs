//! Timeline model for the engine.
//!
//! The project file is OpenTimelineIO JSON. These are hand-written serde types
//! (no maintained Rust OTIO implementation exists); CI round-trips fixtures
//! through the Python `opentimelineio` library to prove they are valid OTIO.
//!
//! Rules (see ARCHITECTURE.md, "Project format"):
//! - Time is always exact rational time. No floats in the model.
//! - Every addressable thing has a stable ID; commands address IDs, never indexes.
//! - Our extensions live under one `metadata.ve` key so other OTIO tools ignore them.

pub mod curve;
pub mod doc;
pub mod id;
pub mod meta;
pub mod migrate;
pub mod otio;
pub mod summary;
pub mod text;
pub mod time;
pub mod timecode;
pub mod transcript;

pub use id::Id;
pub use meta::Meta;
pub use otio::{Clip, Composable, ExternalReference, Gap, Marker, MediaReference, Stack, Timeline, Track, TrackKind, Transition};
pub use time::{Rate, RationalTime, TimeRange};

/// Current project schema version, stored in `metadata.ve.project`.
pub const SCHEMA_VERSION: semver::Version = semver::Version::new(0, 1, 0);

/// Metadata namespace key. Placeholder until the product name is decided.
pub const NAMESPACE: &str = "ve";
