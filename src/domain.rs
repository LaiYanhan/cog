// Domain types — the core data model.

pub mod assertion;
pub mod changelog;
pub mod display;
pub mod entity;
pub mod evidence;
pub mod grounds;
pub mod metrics;
pub mod naming;
pub mod relations;
pub mod report;
pub mod risk;

// Re-export core types for convenience
pub use assertion::{Assertion, AssertionKind, AssertionStatus, short_id};
pub use changelog::{ChangelogAction, ChangelogEntry};
pub use display::{AssertedEntity, MAX_ASSERTED, entities_word, partition_by_assertion, plural_s};
pub use entity::{Entity, EntityKind, EntityOrigin};
pub use evidence::Evidence;
pub use metrics::EntityMetrics;
pub use naming::{ancestors, last_segment, normalize, parent_qname, path_to_qualified};
pub use relations::{
    AssertionRelation, AssertionRelationKind, EntityRelation, EntityRelationKind, RelatedEntity,
    RelationDirection,
};
pub use report::{
    ActiveExperiment, AffectedAssertion, CascadeReason, CascadeReport, EntityIndex, ExportFormat,
    ImpactCard, IndexCoverage, ModelSnapshot, ModelStats, ModuleCoverage, NextModelSummary,
    NextReport, NextSuggestion, QueryCard, ScoutSuggestion, StatusMessage, SyncReport,
    TopUncovered, TraceAssertion, TraceTree, VerificationIssue, VerificationIssueKind,
    VerificationReport,
};
pub use risk::RiskAssessment;
