use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// EntityOrigin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EntityOrigin {
    #[default]
    Manual,
    Scan,
    /// Created by experiment commit for a not-yet-scanned entity.
    /// Promoted to Scan when cog sync discovers the entity in code.
    Experiment,
}

impl Display for EntityOrigin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::Scan => "scan",
            Self::Experiment => "experiment",
        })
    }
}

impl FromStr for EntityOrigin {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Self::Manual),
            "scan" => Ok(Self::Scan),
            "experiment" => Ok(Self::Experiment),
            _ => Err("invalid entity origin"),
        }
    }
}

// ---------------------------------------------------------------------------
// EntityKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Module,
    Function,
    Type,
    Field,
    Method,
    Unknown,
}

impl Display for EntityKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Module => "module",
            Self::Function => "function",
            Self::Type => "type",
            Self::Field => "field",
            Self::Method => "method",
            Self::Unknown => "unknown",
        })
    }
}

impl FromStr for EntityKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "module" => Ok(Self::Module),
            "function" => Ok(Self::Function),
            "type" => Ok(Self::Type),
            "field" => Ok(Self::Field),
            "method" => Ok(Self::Method),
            "unknown" => Ok(Self::Unknown),
            _ => Err("invalid entity kind"),
        }
    }
}

// ---------------------------------------------------------------------------
// EntityKind
// ---------------------------------------------------------------------------

impl EntityKind {
    pub fn infer(qualified_name: &str) -> Self {
        let symbol = super::naming::last_segment(qualified_name);
        if symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
            EntityKind::Type
        } else if qualified_name.contains(super::naming::SEP) {
            EntityKind::Function
        } else {
            EntityKind::Module
        }
    }
}

// ---------------------------------------------------------------------------
// Entity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    pub qualified_name: String,
    pub kind: EntityKind,
    pub origin: EntityOrigin,
    #[serde(default)]
    pub metrics: crate::domain::metrics::EntityMetrics,
    pub created_at: DateTime<Utc>,
}

impl Entity {
    /// Create an entity from a tree-sitter scan. Origin is automatically Scan.
    pub fn from_scan(
        name: String,
        kind: EntityKind,
        metrics: crate::domain::metrics::EntityMetrics,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            qualified_name: name,
            kind,
            origin: EntityOrigin::Scan,
            metrics,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_uppercase_is_type() {
        assert_eq!(EntityKind::infer("MyClass"), EntityKind::Type);
    }

    #[test]
    fn infer_qualified_uppercase_is_type() {
        assert_eq!(
            EntityKind::infer("cog::repo::SqliteRepository"),
            EntityKind::Type
        );
    }

    #[test]
    fn infer_qualified_lowercase_is_function() {
        assert_eq!(EntityKind::infer("cog::repo::open"), EntityKind::Function);
    }

    #[test]
    fn infer_bare_lowercase_is_module() {
        assert_eq!(EntityKind::infer("utils"), EntityKind::Module);
    }
}
