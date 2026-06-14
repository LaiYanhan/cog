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
    #[serde(default)]
    pub covered_entities: u64,
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
    /// Manual-origin entity with assertions but no relations —
    /// likely an orphan from a `cog assert` that used a short name
    /// before entity resolution was fixed.
    OrphanManualEntity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationIssue {
    pub kind: VerificationIssueKind,
    pub entity_name: Option<String>,
    pub assertion_id: Option<String>,
    pub detail: String,
}

impl VerificationIssue {
    /// Construct an issue scoped to an entity (and optionally an assertion).
    pub fn new(
        kind: VerificationIssueKind,
        entity_name: &str,
        assertion_id: Option<&str>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            entity_name: Some(entity_name.to_string()),
            assertion_id: assertion_id.map(str::to_string),
            detail: detail.into(),
        }
    }
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
    /// Assertion count per downstream entity (same order as downstream_entities).
    pub downstream_assertion_counts: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_assessment: Option<crate::domain::RiskAssessment>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub downstream_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blind_downstream: Option<usize>,
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
    /// Map of (entity_id -> active_assertion_count) for each related target.
    pub related_assertion_counts: HashMap<String, usize>,
    /// When true, render full relation details instead of the summary.
    pub relations_detail: bool,
}

/// Result of a `cog index` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityIndex {
    pub entities: Vec<(Entity, usize)>,
    #[serde(default)]
    pub summary_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub coverage_summary: Option<IndexCoverage>,
}

/// Result of a `cog sync` command — the sole codebase-scanning command.
///
/// Replaces the old `cog init` (one-shot) and `cog sync` (incremental) with a
/// single idempotent command that always runs a full scan, creating entities and
/// relations, and removing stale Scan-origin entities (unless they have assertions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    pub files_scanned: usize,
    pub files_by_language: HashMap<String, usize>,
    pub entities_created: usize,
    pub entities_removed: usize,
    pub relations_created: usize,
    pub entity_counts_by_kind: HashMap<String, usize>,
    /// Stale entities that were removed.
    pub stale_entities: Vec<String>,
    /// Stale entities that were NOT removed (they have assertions).
    pub stale_skipped: Vec<String>,
    pub dry_run: bool,
    /// Whether the scan detected any drift (new, stale, or skipped entities).
    pub has_drift: bool,
    pub after_entities: usize,
    pub after_assertions: usize,
    /// Assertions on stale-skipped entities — these may need review.
    /// Each entry is (entity_name, assertion).
    #[serde(default)]
    pub affected_assertions: Vec<(String, Assertion)>,
    /// Provisional entities (origin=Experiment) not found in the codebase.
    /// These were created by experiment commit but the agent hasn't implemented
    /// the corresponding code yet. Advisory — not auto-deleted.
    #[serde(default)]
    pub unresolved_provisional: Vec<String>,
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

/// Suggestion from the scout subcommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoutSuggestion {
    pub entity_name: String,
    pub entity_kind: String,
    pub reason: String,
    pub action: ScoutAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScoutAction {
    Assert,
}

/// A currently active (Open/Evaluated) experiment detected on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveExperiment {
    pub short_id: String,
    pub description: String,
    /// "draft" or "evaluated"
    pub status: String,
    /// File modification time of the experiment JSON — used as a proxy for
    /// when the experiment was last evaluated. `None` if unavailable.
    pub mtime: Option<chrono::DateTime<chrono::Utc>>,
}

/// Result of a `cog next` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextReport {
    pub state: String,
    pub active_experiments: Vec<ActiveExperiment>,
    pub model: NextModelSummary,
    pub covered: u64,
    pub coverage_pct: f64,
    pub suggestions: Vec<NextSuggestion>,
    pub stagnation_warning: Option<String>,
    /// Provisional entities (origin=Experiment) not found in codebase.
    #[serde(default)]
    pub unresolved_provisional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextModelSummary {
    pub entities: u64,
    pub assertions: u64,
    pub active: u64,
    pub retracted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextSuggestion {
    pub kind: String,
    pub description: String,
    pub next_command: String,
}

/// Coverage breakdown for the index summary view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexCoverage {
    pub covered: usize,
    pub total: usize,
    pub pct: f64,
    pub modules: Vec<ModuleCoverage>,
    pub top_uncovered: Vec<TopUncovered>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCoverage {
    pub path: String,
    pub covered: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopUncovered {
    pub entity_name: String,
    pub entity_kind: String,
    pub assertions: usize,
    pub dependents: usize,
}

/// Lightweight status message for commands with simple output (assert, depend, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMessage {
    pub message: String,
}
