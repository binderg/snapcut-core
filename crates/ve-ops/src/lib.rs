//! Edit commands as pure functions.
//!
//! Every command is `(doc, params) -> Result<Applied>`: the new document, the
//! forward JSON Patch (RFC 6902) emitted as `project.changed`, and the inverse
//! patch stored for undo. No I/O here, which is what makes property tests
//! ("applying the inverse restores the original") cheap.
//!
//! Plugin callbacks run inside a transaction (`txn_id`) so a plugin's many
//! edits commit as one revision and one undo group.

/// Output of a committed (or dry-run) command.
#[derive(Debug, Clone)]
pub struct Applied<D> {
    pub doc: D,
    pub patch: json_patch::Patch,
    pub inverse: json_patch::Patch,
}

/// Derive forward and inverse patches by diffing before/after documents.
pub fn diff_patches(before: &serde_json::Value, after: &serde_json::Value) -> (json_patch::Patch, json_patch::Patch) {
    (json_patch::diff(before, after), json_patch::diff(after, before))
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
