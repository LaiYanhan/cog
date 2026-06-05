// Domain types — the core data model.
// Methods marked #[allow(dead_code)] are forward-looking and will be wired as features land.

pub mod assertion;
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
pub use entity::{Entity, EntityKind, EntityOrigin, parent_qname};
pub use evidence::Evidence;
pub use relations::{
    AssertionRelation, AssertionRelationKind, EntityRelation, EntityRelationKind, RelatedEntity,
    RelationDirection,
};
pub use report::{
    AffectedAssertion, CascadeReason, CascadeReport, ExportFormat, ImpactCard, ModelSnapshot,
    ModelStats, TraceAssertion, TraceTree, VerificationIssue, VerificationIssueKind,
};
