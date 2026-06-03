use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entity {
    pub id: String,
    pub qualified_name: String,
    pub kind: EntityKind,
    pub origin: EntityOrigin,
    pub created_at: DateTime<Utc>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assertion {
    pub id: String,
    pub entity_id: String,
    pub kind: AssertionKind,
    pub claim: String,
    pub status: AssertionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retraction_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    Contract,
    Intent,
    Invariant,
    Fragility,
    Correction,
}

impl Display for AssertionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Contract => "contract",
            Self::Intent => "intent",
            Self::Invariant => "invariant",
            Self::Fragility => "fragility",
            Self::Correction => "correction",
        })
    }
}

impl FromStr for AssertionKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contract" => Ok(Self::Contract),
            "intent" => Ok(Self::Intent),
            "invariant" => Ok(Self::Invariant),
            "fragility" => Ok(Self::Fragility),
            "correction" => Ok(Self::Correction),
            _ => Err("invalid assertion kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    Active,
    Retracted,
    Uncertain,
}

impl Display for AssertionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Active => "active",
            Self::Retracted => "retracted",
            Self::Uncertain => "uncertain",
        })
    }
}

impl FromStr for AssertionStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "retracted" => Ok(Self::Retracted),
            "uncertain" => Ok(Self::Uncertain),
            _ => Err("invalid assertion status"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub id: String,
    pub assertion_id: String,
    pub source: String,
    pub detail: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum EntityRelationKind {
    Contains,
    Calls,
    Uses,
}

impl Display for EntityRelationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Contains => "contains",
            Self::Calls => "calls",
            Self::Uses => "uses",
        })
    }
}

impl FromStr for EntityRelationKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contains" => Ok(Self::Contains),
            "calls" => Ok(Self::Calls),
            "uses" => Ok(Self::Uses),
            _ => Err("invalid entity relation kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssertionRelationKind {
    DependsOn,
}

impl Display for AssertionRelationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("depends_on")
    }
}

impl FromStr for AssertionRelationKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "depends_on" => Ok(Self::DependsOn),
            _ => Err("invalid assertion relation kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangelogAction {
    Assert,
    Retract,
    CascadeMark,
    Depend,
    Verify,
    DeleteEntity,
}

impl Display for ChangelogAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Assert => "assert",
            Self::Retract => "retract",
            Self::CascadeMark => "cascade_mark",
            Self::Depend => "depend",
            Self::Verify => "verify",
            Self::DeleteEntity => "delete_entity",
        })
    }
}

impl FromStr for ChangelogAction {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "assert" => Ok(Self::Assert),
            "retract" => Ok(Self::Retract),
            "cascade_mark" => Ok(Self::CascadeMark),
            "depend" => Ok(Self::Depend),
            "verify" => Ok(Self::Verify),
            "delete_entity" => Ok(Self::DeleteEntity),
            _ => Err("invalid changelog action"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangelogEntry {
    pub id: String,
    pub action: ChangelogAction,
    pub target_id: String,
    pub detail: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelatedEntity {
    pub entity: Entity,
    pub kind: EntityRelationKind,
    pub direction: RelationDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityRelation {
    pub id: String,
    pub from_entity: String,
    pub to_entity: String,
    pub kind: EntityRelationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssertionRelation {
    pub id: String,
    pub from_assertion: String,
    pub to_assertion: String,
    pub kind: AssertionRelationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelStats {
    pub entities: u64,
    pub assertions: u64,
    pub active_assertions: u64,
    pub uncertain_assertions: u64,
    pub retracted_assertions: u64,
    pub evidences: u64,
    pub corrections: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Toml,
    Dot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationIssueKind {
    IsolatedEntity,
    MissingEvidence,
    DependencyOnRetracted,
    DependencyOnUncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationIssue {
    pub kind: VerificationIssueKind,
    pub entity_name: Option<String>,
    pub assertion_id: Option<String>,
    pub detail: String,
}

/// Complete snapshot of the cognitive model, used for diff/merge operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSnapshot {
    pub entities: Vec<Entity>,
    pub assertions: Vec<Assertion>,
    pub evidences: Vec<Evidence>,
    pub entity_relations: Vec<EntityRelation>,
    pub assertion_relations: Vec<AssertionRelation>,
    pub changelog: Vec<ChangelogEntry>,
}
