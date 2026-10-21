//! Plugins.
//!
//! A plugin is a folder with `plugin.json`. Process plugins are spawned and
//! spoken to over stdio as newline-delimited JSON-RPC (tokio-util LinesCodec);
//! logs go to stderr. Plugins call back into the engine as actor `plugin:<id>`
//! inside the caller's transaction. Discovery order: project `plugins/`, user
//! plugin folder, built-ins. Secrets (BYOK keys) come from the OS keychain at
//! spawn time, never from the project file.

use serde::{Deserialize, Serialize};

/// `plugin.json` (subset; grows with the plugin phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub version: semver::Version,
    /// Supported engine range, e.g. `">=0.4, <0.6"`.
    pub engine: semver::VersionReq,
    pub protocol: String,
    pub name: String,
}

impl Manifest {
    pub fn supports(&self, engine: &semver::Version) -> bool {
        self.engine.matches(engine)
    }
}
