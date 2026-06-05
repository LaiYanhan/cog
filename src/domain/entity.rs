use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// EntityOrigin
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// EntityOrigin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EntityOrigin {
    #[default]
    Manual,
    Scan,
}

impl Display for EntityOrigin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::Scan => "scan",
        })
    }
}

impl FromStr for EntityOrigin {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Self::Manual),
            "scan" => Ok(Self::Scan),
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

impl EntityKind {
    pub fn infer(qualified_name: &str) -> Self {
        let symbol = qualified_name.rsplit("::").next().unwrap_or(qualified_name);
        if symbol.chars().next().is_some_and(|c| c.is_uppercase()) {
            EntityKind::Type
        } else if qualified_name.contains("::") {
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

#[allow(dead_code)]
impl Entity {
    pub fn from_scan(name: &str, kind: EntityKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            qualified_name: name.to_string(),
            kind,
            origin: EntityOrigin::Scan,
            metrics: crate::domain::metrics::EntityMetrics::empty(),
            created_at: Utc::now(),
        }
    }
    pub fn from_manual(name: &str, kind: EntityKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            qualified_name: name.to_string(),
            kind,
            origin: EntityOrigin::Manual,
            metrics: crate::domain::metrics::EntityMetrics::empty(),
            created_at: Utc::now(),
        }
    }
    pub fn short_name(&self) -> &str {
        self.qualified_name
            .rsplit("::")
            .next()
            .unwrap_or(&self.qualified_name)
    }

    pub fn module(&self) -> Option<&str> {
        self.qualified_name.rsplit_once("::").map(|(p, _)| p)
    }

    pub fn is_public(&self) -> bool {
        // Symbols that don't start with '_' are considered public.
        self.short_name().chars().next().is_some_and(|c| c != '_')
    }
}
