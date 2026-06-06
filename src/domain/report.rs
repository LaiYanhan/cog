use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::domain::relations::RelatedEntity;
use crate::domain::{
    Assertion, AssertionRelation, ChangelogEntry, Entity, EntityRelation, Evidence,
};

// ---------------------------------------------------------------------------
// Misc / stats types (from model/types.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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

impl Display for ExportFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Json => write!(f, "json"),
            ExportFormat::Toml => write!(f, "toml"),
            ExportFormat::Dot => write!(f, "dot"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationIssueKind {
    IsolatedEntity,
    MissingEvidence,
    DependencyOnRetracted,
    DependencyOnUncertain,
    DanglingGrounds,
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

// ---------------------------------------------------------------------------
// Report types (new)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CascadeReport {
    pub retracted: Assertion,
    pub affected: Vec<AffectedAssertion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AffectedAssertion {
    pub assertion: Assertion,
    pub cascade_reason: CascadeReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CascadeReason {
    MarkedUncertain,
    GroundWeakened,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImpactCard {
    pub entity: Entity,
    pub downstream_entities: Vec<Entity>,
    pub affected_assertions: Vec<Assertion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_assessment: Option<crate::space::risk::RiskAssessment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceTree {
    pub entity: Entity,
    pub assertions: Vec<TraceAssertion>,
    pub related_entities: Vec<RelatedEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceAssertion {
    pub assertion: Assertion,
    pub evidences: Vec<Evidence>,
    pub dependencies: Vec<TraceAssertion>,
}

// ---------------------------------------------------------------------------
// Command report types — used with emit_report for text/json output routing
// ---------------------------------------------------------------------------

/// Result of a `cog query` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCard {
    pub entity: Entity,
    pub assertions: Vec<(Assertion, Vec<Evidence>)>,
    pub related: Vec<RelatedEntity>,
}

/// Result of a `cog index` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityIndex {
    pub entities: Vec<(Entity, usize)>,
}

/// Result of a `cog init` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitReport {
    pub files_scanned: usize,
    pub files_by_language: HashMap<String, usize>,
    pub entities_created: usize,
    pub relations_created: usize,
    pub entity_counts_by_kind: HashMap<String, usize>,
    pub dry_run: bool,
}

/// Result of a `cog verify` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub checked_count: usize,
    pub issues: Vec<VerificationIssue>,
    pub cleaned_count: usize,
    pub scan_issues: Vec<String>,
    pub success: bool,
}

/// Lightweight status message for commands with simple output (assert, depend, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub message: String,
}
