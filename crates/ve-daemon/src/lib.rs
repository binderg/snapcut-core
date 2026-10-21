//! Session server.
//!
//! One daemon per project folder (file lock). The CLI connects over a local
//! socket / Windows named pipe and spawns the daemon if none is running; the
//! daemon exits after an idle timeout. It owns the in-memory project, decoders,
//! frame cache, jobs and the `project.changed` event stream. Caches and proxies
//! live in the OS cache dir, not the (possibly OneDrive-synced) project folder.
//!
//! This crate also wires every method into the shared [`ve_protocol::Registry`].

use std::sync::Arc;

use ve_protocol::{NoParams, PROTOCOL_VERSION, Registry, VersionInfo};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build the method registry used by every transport.
pub fn build_registry() -> Arc<Registry> {
    let mut r = Registry::new();
    r.method("system.version", "Engine, protocol and project schema versions", false, |_: NoParams| {
        Ok(VersionInfo {
            engine: ENGINE_VERSION.into(),
            protocol: PROTOCOL_VERSION,
            schema: ve_model::SCHEMA_VERSION.to_string(),
        })
    });
    r.seal("Headless Video Engine", ENGINE_VERSION)
}
