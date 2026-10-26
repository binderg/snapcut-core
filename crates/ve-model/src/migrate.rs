//! Project format versions and upgrades.
//!
//! `metadata.ve.project.schema_version` says which shape of `metadata.ve` a
//! file has. Opening a file:
//!
//! - **Same version:** read as is.
//! - **Older:** each step in [`STEPS`] upgrades the raw JSON one version, in
//!   order, before it is read into the model. The file on disk is not
//!   touched until the next save writes the new version.
//! - **Newer:** refused ([`Check::TooNew`]). Before 1.0 a newer minor counts
//!   (0.2 may be breaking for a 0.1 engine); from 1.0 only a newer major.
//!
//! **When to bump [`crate::SCHEMA_VERSION`].** Whenever an older engine
//! would misread a file or drop data from it on save. A new optional field
//! is such a change (an older engine ignores it and loses it when it saves),
//! so before 1.0 it bumps the minor version. Renaming or reshaping a field
//! also needs a step here that rewrites old files. Changes that only add
//! accepted values to an existing field (a new blend mode, a new ease) bump
//! the patch version, which every engine of the same minor reads.
//!
//! Steps work on `serde_json::Value`, not the model types, so they keep
//! working after the types move on.

use semver::Version;
use serde_json::Value;

use crate::SCHEMA_VERSION;

/// One upgrade: files at `from` (major.minor; any patch) become `to`.
pub struct Step {
    pub from: (u64, u64),
    pub to: Version,
    /// What changes, for the changelog and errors.
    pub what: &'static str,
    pub apply: fn(&mut Value) -> Result<(), String>,
}

/// Every upgrade, oldest first. Empty while no released engine wrote an
/// older format.
pub const STEPS: &[Step] = &[];

/// How a file's version relates to this engine's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Current,
    /// Readable after the upgrade steps run.
    Older,
    TooNew,
    /// Older, but no step leads from it to the current version.
    NoPath,
}

/// Compare a file's version with `current` (this engine's) given `steps`.
pub fn check(found: &Version, current: &Version, steps: &[Step]) -> Check {
    let too_new = if current.major == 0 { found.major > 0 || found.minor > current.minor } else { found.major > current.major };
    if too_new {
        return Check::TooNew;
    }
    if same_line(found, current) {
        return Check::Current;
    }
    let mut v = found.clone();
    while !same_line(&v, current) {
        match steps.iter().find(|s| s.from == (v.major, v.minor)) {
            Some(s) => v = s.to.clone(),
            None => return Check::NoPath,
        }
    }
    Check::Older
}

/// Versions an engine reads without upgrading: same major (and before 1.0,
/// same minor).
fn same_line(a: &Version, b: &Version) -> bool {
    a.major == b.major && (b.major > 0 || a.minor == b.minor)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MigrateError {
    #[error("no metadata.ve.project.schema_version")]
    NoVersion,
    #[error("schema_version `{0}` is not a version like 0.1.0")]
    BadVersion(String),
    #[error("saved by a newer engine (schema {found}; this engine reads up to {supported})")]
    TooNew { found: Version, supported: Version },
    #[error("schema {found} is too old for this engine (it reads {supported}) and there is no upgrade from it")]
    NoPath { found: Version, supported: Version },
    #[error("upgrading from schema {from} to {to} ({what}) failed: {message}")]
    Step { from: Version, to: Version, what: &'static str, message: String },
}

/// Upgrade `doc` (a whole `project.otio`) to this engine's version.
/// Returns the version it had when steps ran, `None` when it was current.
pub fn migrate(doc: &mut Value) -> Result<Option<Version>, MigrateError> {
    migrate_with(doc, &SCHEMA_VERSION, STEPS)
}

pub fn migrate_with(doc: &mut Value, current: &Version, steps: &[Step]) -> Result<Option<Version>, MigrateError> {
    let found = version_of(doc)?;
    match check(&found, current, steps) {
        Check::Current => Ok(None),
        Check::TooNew => Err(MigrateError::TooNew { found, supported: current.clone() }),
        Check::NoPath => Err(MigrateError::NoPath { found, supported: current.clone() }),
        Check::Older => {
            let mut v = found.clone();
            while !same_line(&v, current) {
                let s = steps.iter().find(|s| s.from == (v.major, v.minor)).expect("check found a path");
                (s.apply)(doc).map_err(|message| MigrateError::Step { from: v.clone(), to: s.to.clone(), what: s.what, message })?;
                v = s.to.clone();
            }
            set_version(doc, current);
            Ok(Some(found))
        }
    }
}

fn version_of(doc: &Value) -> Result<Version, MigrateError> {
    let raw = doc.pointer("/metadata/ve/project/schema_version").and_then(Value::as_str).ok_or(MigrateError::NoVersion)?;
    Version::parse(raw).map_err(|_| MigrateError::BadVersion(raw.to_owned()))
}

fn set_version(doc: &mut Value, v: &Version) {
    if let Some(p) = doc.pointer_mut("/metadata/ve/project") {
        p["schema_version"] = Value::String(v.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(v: &str) -> Value {
        json!({"metadata": {"ve": {"project": {"schema_version": v, "revision": 3}}}, "tracks": {"children": []}})
    }

    fn rename_speed(d: &mut Value) -> Result<(), String> {
        d["renamed"] = json!(true);
        Ok(())
    }

    fn add_flag(d: &mut Value) -> Result<(), String> {
        if d.get("renamed").is_none() {
            return Err("steps ran out of order".into());
        }
        d["flag"] = json!(1);
        Ok(())
    }

    fn broken(_: &mut Value) -> Result<(), String> {
        Err("bad clip".into())
    }

    const STEPS: &[Step] = &[
        Step { from: (0, 1), to: Version::new(0, 2, 0), what: "rename", apply: rename_speed },
        Step { from: (0, 2), to: Version::new(0, 3, 0), what: "flag", apply: add_flag },
    ];

    #[test]
    fn current_files_are_left_alone() {
        let mut d = doc("0.3.1");
        assert_eq!(migrate_with(&mut d, &Version::new(0, 3, 0), STEPS), Ok(None));
        assert_eq!(d, doc("0.3.1"), "a newer patch of the same minor reads as is");
    }

    #[test]
    fn older_files_run_every_step_in_order() {
        let mut d = doc("0.1.4");
        assert_eq!(migrate_with(&mut d, &Version::new(0, 3, 0), STEPS), Ok(Some(Version::new(0, 1, 4))));
        assert_eq!((d["renamed"].clone(), d["flag"].clone()), (json!(true), json!(1)));
        assert_eq!(d["metadata"]["ve"]["project"]["schema_version"], "0.3.0");
        assert_eq!(d["metadata"]["ve"]["project"]["revision"], 3, "the rest is kept");
    }

    #[test]
    fn newer_missing_and_failing_are_errors() {
        let cur = Version::new(0, 3, 0);
        assert!(matches!(migrate_with(&mut doc("0.4.0"), &cur, STEPS), Err(MigrateError::TooNew { .. })));
        assert!(matches!(migrate_with(&mut doc("1.0.0"), &cur, STEPS), Err(MigrateError::TooNew { .. })));
        assert!(matches!(migrate_with(&mut doc("0.0.9"), &cur, STEPS), Err(MigrateError::NoPath { .. })));
        assert!(matches!(migrate_with(&mut json!({}), &cur, STEPS), Err(MigrateError::NoVersion)));
        assert!(matches!(migrate_with(&mut doc("one"), &cur, STEPS), Err(MigrateError::BadVersion(_))));
        let bad = [Step { from: (0, 2), to: Version::new(0, 3, 0), what: "x", apply: broken }];
        let e = migrate_with(&mut doc("0.2.0"), &cur, &bad).unwrap_err();
        assert!(e.to_string().contains("bad clip"), "{e}");
    }

    #[test]
    fn after_one_point_oh_only_majors_matter() {
        let cur = Version::new(1, 4, 0);
        assert_eq!(check(&Version::new(1, 9, 0), &cur, &[]), Check::Current);
        assert_eq!(check(&Version::new(2, 0, 0), &cur, &[]), Check::TooNew);
        assert_eq!(check(&Version::new(0, 9, 0), &cur, &[]), Check::NoPath);
    }

    #[test]
    fn the_shipped_steps_chain_to_this_version() {
        for s in super::STEPS {
            assert!(s.to <= SCHEMA_VERSION, "{} goes past {SCHEMA_VERSION}", s.what);
            let mut v = s.to.clone();
            while !same_line(&v, &SCHEMA_VERSION) {
                v = super::STEPS.iter().find(|n| n.from == (v.major, v.minor)).unwrap_or_else(|| panic!("no step from {v}")).to.clone();
            }
        }
    }
}
