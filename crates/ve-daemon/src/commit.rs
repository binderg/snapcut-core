//! The one path every mutating method takes: check `base_rev`, run the pure
//! command, diff, then either return the patch (dry run) or bump the
//! revision, save, and record the change in history under its actor.

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use ve_model::time::Q;
use ve_model::{Id, Rate, Timeline, timecode};
use ve_protocol::{Actor, ActorKind, ErrorCode, RpcError};

use crate::project::OpenProject;

/// The common fields of a mutating call.
pub struct Mutation<'a> {
    pub base_rev: u64,
    pub dry_run: bool,
    pub actor: Option<&'a Actor>,
}

/// Declare a mutating method's params: the given fields plus `base_rev`,
/// `dry_run` and `actor`, with unknown fields rejected.
macro_rules! mutation_params {
    ($(#[$meta:meta])* pub struct $name:ident { $($(#[$fm:meta])* pub $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $($(#[$fm])* pub $field: $ty,)*
            /// The revision this call was planned against; a mismatch fails with `stale_revision`.
            pub base_rev: u64,
            /// Return the patch the call would apply without changing the project.
            #[serde(default)]
            pub dry_run: bool,
            /// Who is editing (`{"kind": "agent", "id": "claude"}`); defaults to the local user.
            #[serde(default)]
            pub actor: Option<ve_protocol::Actor>,
        }

        impl $name {
            pub fn mutation(&self) -> $crate::commit::Mutation<'_> {
                $crate::commit::Mutation { base_rev: self.base_rev, dry_run: self.dry_run, actor: self.actor.as_ref() }
            }
        }
    };
}
pub(crate) use mutation_params;

/// Result of a mutating call: the shared fields plus the command's own result.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Committed<T> {
    /// Project revision after the call (unchanged on dry runs and no-ops).
    pub revision: u64,
    pub dry_run: bool,
    /// Whether the call changed (or, on a dry run, would change) the project.
    pub changed: bool,
    /// JSON Patch (RFC 6902) from the previous revision, as `project.changed` carries it.
    pub patch: Value,
    /// The undo group this edit joined, if the actor has one open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<Id>,
    #[serde(flatten)]
    pub result: T,
}

pub fn actor_label(a: Option<&Actor>) -> String {
    let a = a.cloned().unwrap_or_else(Actor::user);
    let kind = match a.kind {
        ActorKind::User => "user",
        ActorKind::Agent => "agent",
        ActorKind::Plugin => "plugin",
    };
    format!("{kind}:{}", a.id)
}

fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::Internal, e.to_string())
}

/// Run `f` against the open project and commit its result.
pub fn commit<T>(
    project: &mut OpenProject,
    m: Mutation<'_>,
    command: &str,
    f: impl FnOnce(&Timeline) -> Result<(Timeline, T), RpcError>,
) -> Result<Committed<T>, RpcError> {
    let current = project.revision();
    if m.base_rev != current {
        return Err(RpcError::stale_revision(current));
    }
    let (next, result) = f(&project.timeline)?;
    commit_doc(project, m, command, next, result)
}

/// Commit an already-built next document (undo and redo build theirs from patches).
pub fn commit_doc<T>(
    project: &mut OpenProject,
    m: Mutation<'_>,
    command: &str,
    mut next: Timeline,
    result: T,
) -> Result<Committed<T>, RpcError> {
    let current = project.revision();
    let applied = ve_ops::commit_patches(&project.timeline, &next).map_err(internal)?;
    let actor = actor_label(m.actor);
    let group = project.history.open_group(&actor).map(|g| g.id.clone());
    if applied.patch.0.is_empty() {
        return Ok(Committed { revision: current, dry_run: m.dry_run, changed: false, patch: json!([]), group: None, result });
    }
    let rev = current + 1;
    let mut patch = serde_json::to_value(&applied.patch).map_err(internal)?;
    if let Some(ops) = patch.as_array_mut() {
        ops.push(json!({"op": "replace", "path": "/metadata/ve/project/revision", "value": rev}));
    }
    if m.dry_run {
        return Ok(Committed { revision: current, dry_run: true, changed: true, patch, group, result });
    }
    let meta = next
        .metadata
        .ve
        .project
        .as_mut()
        .ok_or_else(|| RpcError::invalid_params("the project has no metadata.ve.project block"))?;
    meta.revision = rev;
    project.commit(next).map_err(crate::rpc_error)?;
    if command == "history.undo" {
        project.history.undone(rev);
    } else if command == "history.redo" {
        project.history.redone(rev);
    } else {
        project.history.record(rev, &actor, command, applied.patch, applied.inverse);
    }
    project.save_history();
    Ok(Committed { revision: rev, dry_run: false, changed: true, patch, group, result })
}

/// Parse a time someone typed (`00:01:12:12`, `f2172`, `72.4s`, `+15f`, `-1s`) at `rate`, as seconds.
pub fn parse_time(field: &str, s: &str, rate: Rate) -> Result<Q, RpcError> {
    let t = s.trim();
    let t = t.strip_prefix('+').unwrap_or(t);
    timecode::parse(t, rate).map(|t| t.seconds()).map_err(|e| RpcError::invalid_params(format!("{field}: {e}")))
}

/// Parse an exact ratio: `3/2`, `1.5`, `2`.
pub fn parse_ratio(field: &str, s: &str) -> Result<Q, RpcError> {
    let bad = || RpcError::invalid_params(format!("{field} `{s}`: use a fraction like 3/2 or a decimal like 1.5"));
    let s = s.trim().trim_end_matches(['x', 'X']);
    if let Some((n, d)) = s.split_once('/') {
        let (n, d): (i64, i64) = (n.trim().parse().map_err(|_| bad())?, d.trim().parse().map_err(|_| bad())?);
        if d == 0 {
            return Err(bad());
        }
        return Ok(Q::new(n, d));
    }
    // An exact decimal, via the timecode parser's seconds form at rate 1.
    timecode::parse(&format!("{s}s"), Rate::integer(1)).map(|t| t.value).map_err(|_| bad())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ratios_and_times() {
        assert_eq!(parse_ratio("speed", "3/2").unwrap(), Q::new(3, 2));
        assert_eq!(parse_ratio("speed", "1.25").unwrap(), Q::new(5, 4));
        assert_eq!(parse_ratio("speed", "2x").unwrap(), Q::from_integer(2));
        assert!(parse_ratio("speed", "fast").is_err());
        let r = Rate::integer(30);
        assert_eq!(parse_time("at", "+15f", r).unwrap(), Q::new(1, 2));
        assert_eq!(parse_time("at", "-1s", r).unwrap(), Q::from_integer(-1));
        assert_eq!(parse_time("at", "00:00:01:15", r).unwrap(), Q::new(3, 2));
    }
}
