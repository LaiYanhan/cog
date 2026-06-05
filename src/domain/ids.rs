use std::fmt::{self, Display, Formatter};
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EntityId
// ---------------------------------------------------------------------------

/// A UUID identifying an entity in the cognitive model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// First 8 characters for display.
    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

impl Display for EntityId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for EntityId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for EntityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for EntityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ---------------------------------------------------------------------------
// AssertionId
// ---------------------------------------------------------------------------

/// A UUID identifying an assertion in the cognitive model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AssertionId(pub String);

impl AssertionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// First 8 characters for display.
    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

impl Display for AssertionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for AssertionId {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for AssertionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for AssertionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ---------------------------------------------------------------------------
// QualifiedName
// ---------------------------------------------------------------------------

/// A `::`-separated path identifying an entity, e.g. `cog::repo::sqlite::SqliteRepository`.
///
/// Invariants: non-empty, each segment is at least 1 character.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QualifiedName(pub String);

impl QualifiedName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Last segment, e.g. `SqliteRepository` from `cog::repo::sqlite::SqliteRepository`.
    pub fn last_segment(&self) -> &str {
        self.0.rsplit("::").next().unwrap_or(&self.0)
    }

    /// Parent path, e.g. `cog::repo::sqlite` from `cog::repo::sqlite::SqliteRepository`.
    pub fn parent(&self) -> Option<&str> {
        self.0.rsplit_once("::").map(|(p, _)| p)
    }

    /// Iterator over path segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split("::")
    }
}

impl Display for QualifiedName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for QualifiedName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for QualifiedName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for QualifiedName {
    fn from(s: String) -> Self {
        Self(s)
    }
}
