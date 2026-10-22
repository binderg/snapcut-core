//! Edit commands as pure functions.
//!
//! Every command is `(doc, params) -> Result<(new doc, report)>`. The caller
//! diffs old and new with [`commit_patches`] to get the forward JSON Patch
//! (RFC 6902) sent as `project.changed` and the inverse stored for undo. No
//! I/O here, which is what makes property tests ("applying the inverse
//! restores the original") cheap.
//!
//! Rules every command follows:
//! - Clips are addressed by stable ID, tracks by ID or name, never by index.
//! - Timeline times snap to the project frame grid (`output.rate`).
//! - Clips that share a link ID (video + its audio) are edited together
//!   unless the call passes `linked: false`.
//! - A command never lets a clip play media its source does not have.
//!
//! Plugin callbacks run inside a transaction (`txn_id`) so a plugin's many
//! edits commit as one revision and one undo group.

pub mod diff;
pub mod edit;
pub mod lane;
pub mod markers;
pub mod tracks;

use serde_json::Value;
use ve_model::Timeline;

pub use edit::{ClipPlace, EditReport};

/// A command that could not be applied. The message is written for the
/// person or agent who made the call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EditError(pub String);

impl EditError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Output of a committed (or dry-run) command.
#[derive(Debug, Clone)]
pub struct Applied<D> {
    pub doc: D,
    pub patch: json_patch::Patch,
    pub inverse: json_patch::Patch,
}

/// Derive forward and inverse patches by diffing before/after documents.
pub fn diff_patches(before: &Value, after: &Value) -> (json_patch::Patch, json_patch::Patch) {
    (diff::diff(before, after), diff::diff(after, before))
}

/// Forward and inverse patches between two versions of a project, leaving
/// out the revision counter (the caller sets that; undo must not restore an
/// old revision number).
pub fn commit_patches(before: &Timeline, after: &Timeline) -> serde_json::Result<Applied<()>> {
    let a = without_revision(serde_json::to_value(before)?);
    let b = without_revision(serde_json::to_value(after)?);
    let (patch, inverse) = diff_patches(&a, &b);
    Ok(Applied { doc: (), patch, inverse })
}

fn without_revision(mut v: Value) -> Value {
    if let Some(p) = v.pointer_mut("/metadata/ve/project") {
        p["revision"] = Value::from(0);
    }
    v
}

/// Apply a patch to a project, checking the result is still a valid document.
pub fn apply_patch(doc: &Timeline, patch: &json_patch::Patch) -> Result<Timeline, EditError> {
    let revision = doc.project().map(|p| p.revision);
    let mut v = serde_json::to_value(doc).map_err(|e| EditError::new(e.to_string()))?;
    json_patch::patch(&mut v, patch).map_err(|e| EditError::new(format!("patch does not apply: {e}")))?;
    let mut next: Timeline =
        serde_json::from_value(v).map_err(|e| EditError::new(format!("patch makes an invalid project: {e}")))?;
    if let (Some(rev), Some(p)) = (revision, next.metadata.ve.project.as_mut()) {
        p.revision = rev;
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inverse_restores_original() {
        let before = json!({"tracks": [{"id": "trk_a", "clips": ["clip_1", "clip_2"]}]});
        let after = json!({"tracks": [{"id": "trk_a", "clips": ["clip_2"]}]});
        let (fwd, inv) = diff_patches(&before, &after);
        let mut doc = before.clone();
        json_patch::patch(&mut doc, &fwd).unwrap();
        assert_eq!(doc, after);
        json_patch::patch(&mut doc, &inv).unwrap();
        assert_eq!(doc, before);
    }
}
