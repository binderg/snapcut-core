//! Short, stable, prefixed IDs such as `clip_7f3a`.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const ALPHABET: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

/// A stable identifier: `<kind>_<hex>`. Kinds are short words such as
/// `clip`, `trk`, `fx`, `kf`, `mk`, `asset`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// New random ID. 6 hex chars gives 16.7M per kind; collisions are
    /// checked by the caller against the document anyway.
    pub fn new(kind: &str) -> Self {
        Self(format!("{kind}_{}", nanoid::nanoid!(6, &ALPHABET)))
    }

    /// An ID from text someone passed in (a param, a file). Not validated:
    /// lookups simply fail for IDs that do not exist.
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The prefix before the first underscore.
    pub fn kind(&self) -> &str {
        self.0.split_once('_').map_or("", |(k, _)| k)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_kind_prefix() {
        let id = Id::new("clip");
        assert_eq!(id.kind(), "clip");
        assert_eq!(id.as_str().len(), "clip_".len() + 6);
    }
}
