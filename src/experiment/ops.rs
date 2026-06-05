use serde::{Deserialize, Serialize};

use crate::domain::{AssertionKind, EntityRelationKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperimentOp {
    HypotheticalAssertion {
        entity_name: String,
        kind: AssertionKind,
        claim: String,
        grounds: String,
        depends_on: Option<String>,
    },
    HypotheticalRetraction {
        assertion_id: String,
        reason: String,
    },
    HypotheticalRelation {
        from_entity: String,
        to_entity: String,
        kind: EntityRelationKind,
    },
}
