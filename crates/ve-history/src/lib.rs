//! Undo/redo history: a stack of committed entries, grouped by actor action.
//!
//! An entry is `(rev, actor, command, patch, inverse)`. `history.begin_group`
//! / `end_group` (and, later, every plugin call) collapse many entries into
//! one undo step. Groups are per actor, so an agent's group never swallows
//! the user's edits.
//!
//! Undo and redo are themselves new revisions: the revision counter only
//! goes up, so optimistic concurrency (`base_rev`) keeps working across them.
//! Only the most recent step can be undone; undoing an older one by a
//! different actor would apply its inverse to a document it was not made
//! against.
//!
//! The history is plain data (serde), so the daemon can persist it next to
//! the project lock and a CLI call in a fresh process still undoes the last
//! one's edit.

use std::collections::BTreeMap;

use json_patch::Patch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Id;

/// Oldest entries are dropped past this many.
pub const MAX_ENTRIES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct HistoryError(pub String);

/// One committed command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Revision this command produced.
    pub rev: u64,
    /// `kind:id`, e.g. `agent:claude`.
    pub actor: String,
    /// Protocol method, e.g. `edit.ripple_delete`.
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
    pub patch: Patch,
    pub inverse: Patch,
}

/// A group an actor has begun and not yet ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenGroup {
    pub id: Id,
    pub label: String,
}

/// One undo step as `history.list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepSummary {
    pub actor: String,
    /// The command, or the group's label for a group of several.
    pub what: String,
    /// Revisions the step's edits produced, oldest first.
    pub revs: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct History {
    /// Revision of the document this history ends at. A project file at
    /// another revision was edited elsewhere, and its history is stale.
    pub head_rev: u64,
    undo: Vec<Entry>,
    /// Undone steps, most recent last; each step's entries oldest first.
    redo: Vec<Vec<Entry>>,
    groups: BTreeMap<String, OpenGroup>,
    /// Labels of groups, open or ended, so summaries can name them.
    #[serde(default)]
    labels: BTreeMap<Id, String>,
}

impl History {
    pub fn new(head_rev: u64) -> Self {
        Self { head_rev, ..Self::default() }
    }

    /// Record a committed command. Joins the actor's open group, if any, and
    /// clears the redo stack. Returns the group it joined.
    pub fn record(&mut self, rev: u64, actor: &str, command: &str, patch: Patch, inverse: Patch) -> Option<Id> {
        let group = self.groups.get(actor).map(|g| g.id.clone());
        self.undo.push(Entry { rev, actor: actor.to_owned(), command: command.to_owned(), group: group.clone(), patch, inverse });
        if self.undo.len() > MAX_ENTRIES {
            let extra = self.undo.len() - MAX_ENTRIES;
            self.undo.drain(..extra);
        }
        self.redo.clear();
        self.head_rev = rev;
        group
    }

    pub fn begin_group(&mut self, actor: &str, id: Id, label: &str) -> Result<OpenGroup, HistoryError> {
        if let Some(g) = self.groups.get(actor) {
            return Err(HistoryError(format!(
                "{actor} already has group {} ({:?}) open; end it first",
                g.id, g.label
            )));
        }
        let g = OpenGroup { id: id.clone(), label: label.to_owned() };
        self.labels.insert(id, label.to_owned());
        self.groups.insert(actor.to_owned(), g.clone());
        Ok(g)
    }

    /// Close the actor's group. Returns it with the number of edits it holds.
    pub fn end_group(&mut self, actor: &str) -> Result<(OpenGroup, usize), HistoryError> {
        let g = self.groups.remove(actor).ok_or_else(|| HistoryError(format!("{actor} has no open group")))?;
        let n = self.undo.iter().filter(|e| e.group.as_ref() == Some(&g.id)).count();
        Ok((g, n))
    }

    pub fn open_group(&self, actor: &str) -> Option<&OpenGroup> {
        self.groups.get(actor)
    }

    /// Open groups by actor.
    pub fn open_groups(&self) -> impl Iterator<Item = (&String, &OpenGroup)> {
        self.groups.iter()
    }

    /// The most recent undo step (entries oldest first): the last entry plus
    /// every entry right before it in the same group.
    pub fn undo_step(&self) -> Option<&[Entry]> {
        let last = self.undo.last()?;
        let n = match &last.group {
            None => 1,
            Some(g) => self.undo.iter().rev().take_while(|e| e.group.as_ref() == Some(g)).count(),
        };
        Some(&self.undo[self.undo.len() - n..])
    }

    pub fn redo_step(&self) -> Option<&[Entry]> {
        self.redo.last().map(Vec::as_slice)
    }

    /// The caller applied the undo step's inverses and committed `new_rev`.
    pub fn undone(&mut self, new_rev: u64) {
        let n = self.undo_step().map_or(0, <[Entry]>::len);
        let step = self.undo.split_off(self.undo.len() - n);
        self.redo.push(step);
        self.head_rev = new_rev;
    }

    /// The caller re-applied the redo step's patches and committed `new_rev`.
    pub fn redone(&mut self, new_rev: u64) {
        if let Some(step) = self.redo.pop() {
            self.undo.extend(step);
        }
        self.head_rev = new_rev;
    }

    /// Undo steps, most recent first.
    pub fn steps(&self, limit: usize) -> Vec<StepSummary> {
        let mut out = Vec::new();
        let mut i = self.undo.len();
        while i > 0 && out.len() < limit {
            let last = &self.undo[i - 1];
            let n = match &last.group {
                None => 1,
                Some(g) => self.undo[..i].iter().rev().take_while(|e| e.group.as_ref() == Some(g)).count(),
            };
            out.push(self.summary(&self.undo[i - n..i]));
            i -= n;
        }
        out
    }

    /// Undone steps that `redo` would bring back, next first.
    pub fn redo_steps(&self) -> Vec<StepSummary> {
        self.redo.iter().rev().map(|s| self.summary(s)).collect()
    }

    pub fn summary(&self, step: &[Entry]) -> StepSummary {
        let first = &step[0];
        let what = first.group.as_ref().and_then(|g| self.labels.get(g)).cloned().unwrap_or_else(|| first.command.clone());
        StepSummary { actor: first.actor.clone(), what, revs: step.iter().map(|e| e.rev).collect(), group: first.group.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Patch {
        Patch(vec![])
    }

    #[test]
    fn groups_collapse_and_are_per_actor() {
        let mut h = History::new(0);
        h.record(1, "user:local", "edit.split", p(), p());
        h.begin_group("agent:claude", Id::from_raw("grp_1"), "remove silences").unwrap();
        h.record(2, "agent:claude", "edit.ripple_delete", p(), p());
        h.record(3, "agent:claude", "edit.ripple_delete", p(), p());
        assert!(h.begin_group("agent:claude", Id::from_raw("grp_2"), "again").is_err());
        let (g, n) = h.end_group("agent:claude").unwrap();
        assert_eq!((g.label.as_str(), n), ("remove silences", 2));

        let steps = h.steps(10);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].what, "remove silences");
        assert_eq!(steps[0].revs, [2, 3]);

        assert_eq!(h.undo_step().unwrap().len(), 2);
        h.undone(4);
        assert_eq!(h.undo_step().unwrap()[0].command, "edit.split");
        assert_eq!(h.redo_steps()[0].revs, [2, 3]);
        h.redone(5);
        assert_eq!(h.undo_step().unwrap().len(), 2);
        assert_eq!(h.head_rev, 5);

        // A new edit clears redo.
        h.undone(6);
        h.record(7, "user:local", "edit.trim", p(), p());
        assert!(h.redo_step().is_none());
    }

    #[test]
    fn round_trips_through_json() {
        let mut h = History::new(3);
        h.begin_group("agent:a", Id::from_raw("grp_1"), "x").unwrap();
        h.record(4, "agent:a", "edit.split", p(), p());
        let back: History = serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
        assert_eq!(back, h);
    }
}
