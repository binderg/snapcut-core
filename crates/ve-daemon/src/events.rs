//! `project.changed`: one JSON-RPC notification per committed revision, plus
//! `job.progress` / `job.finished` for background jobs (see `jobs`).
//!
//! `commit::commit_doc` queues a [`Change`] on the open project; the session
//! drains the queue after each call and broadcasts the notifications. Every
//! transport connection that called `events.subscribe` forwards them. Dry runs
//! and no-op calls send nothing.
//!
//! A client keeps a mirror of the project like this: subscribe (the result
//! carries the current revision), then `project.get`, then apply each
//! `project.changed` patch whose `revision` is newer than the mirror's. A gap
//! in revisions, `events.lagged` or `project.reloaded` (the file was changed
//! outside the engine and re-read) means refetch.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use ve_protocol::Actor;

/// Buffered notifications per subscriber before it counts as lagging.
const CAPACITY: usize = 1024;

/// Params of the `project.changed` notification.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Change {
    /// Revision after the change.
    pub revision: u64,
    pub actor: Actor,
    /// The method that made the change (`edit.trim`, `history.undo`, ...).
    pub command: String,
    /// JSON Patch (RFC 6902) from `revision - 1` to `revision`, the same patch the call returned.
    pub patch: Value,
}

/// The session's event stream. Messages are complete JSON-RPC notifications.
#[derive(Clone)]
pub struct Events {
    tx: broadcast::Sender<Arc<str>>,
}

impl Default for Events {
    fn default() -> Self {
        Self { tx: broadcast::channel(CAPACITY).0 }
    }
}

impl Events {
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.tx.subscribe()
    }

    pub(crate) fn publish(&self, change: &Change) {
        let msg = json!({ "jsonrpc": "2.0", "method": "project.changed", "params": change }).to_string();
        // No subscribers is fine: nobody is watching.
        let _ = self.tx.send(msg.into());
    }

    /// `job.progress` or `job.finished` (see `jobs`).
    pub(crate) fn publish_job(&self, method: &str, params: &Value) {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string();
        let _ = self.tx.send(msg.into());
    }

    /// The file was changed outside the engine and re-read: mirrors must refetch.
    pub(crate) fn publish_reloaded(&self, revision: u64) {
        let msg = json!({ "jsonrpc": "2.0", "method": "project.reloaded", "params": { "revision": revision } }).to_string();
        let _ = self.tx.send(msg.into());
    }
}

/// Sent instead of the events a slow subscriber missed.
pub fn lagged(missed: u64) -> String {
    json!({ "jsonrpc": "2.0", "method": "events.lagged", "params": { "missed": missed } }).to_string()
}
