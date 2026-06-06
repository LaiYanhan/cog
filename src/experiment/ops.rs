use serde::{Deserialize, Serialize};

use crate::domain::{AssertionKind, EntityRelationKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperimentOp {
    Assertion {
        entity_name: String,
        kind: AssertionKind,
        claim: String,
        grounds: String,
        depends_on: Option<String>,
    },
    Retraction {
        assertion_id: String,
        reason: String,
    },
    Relation {
        from_entity: String,
        to_entity: String,
        kind: EntityRelationKind,
    },
    Delete {
        entity_name: String,
    },
}
