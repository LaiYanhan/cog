// Domain types — the core data model.
// Methods marked #[allow(dead_code)] are forward-looking and will be wired as features land.

pub mod assertion;
pub mod ids;
pub mod changelog;
pub mod entity;
pub mod evidence;
pub mod grounds;
pub mod metrics;
pub mod relations;
pub mod report;

// Re-export core types for convenience
pub use assertion::{Assertion, AssertionKind, AssertionStatus};
pub use changelog::{ChangelogAction, ChangelogEntry};
pub use entity::{Entity, EntityKind, EntityOrigin};
pub use evidence::Evidence;
pub use ids::{AssertionId, EntityId, QualifiedName};
pub use metrics::{EntityMetrics, RiskLevel, Visibility};
pub use relations::{
    AssertionRelation, AssertionRelationKind, EntityRelation, EntityRelationKind, RelatedEntity,
    RelationDirection,
};
pub use report::{
    AffectedAssertion, CascadeReason, CascadeReport, ExportFormat, ImpactCard, ModelSnapshot,
    ModelStats, TraceAssertion, TraceTree, VerificationIssue, VerificationIssueKind,
};
