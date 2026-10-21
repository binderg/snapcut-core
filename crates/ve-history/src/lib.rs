//! Undo/redo history: a list of committed entries, grouped by actor action.
//!
//! An entry is `(rev, actor, command, patch, inverse)`. `history.begin_group` /
//! `end_group` (and every plugin call) collapse many entries into one undo step.
