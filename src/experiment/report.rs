use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentReport {
    pub experiment_id: String,
    pub description: String,
    pub entity_focus: String,
    pub ops_count: usize,
    pub risk_score: f64,
    pub affected_count: usize,
    /// Number of assertions that would become Uncertain via TMS cascade.
    pub cascade_count: usize,
    pub contradictions: Vec<Contradiction>,
    /// Assertions affected by the TMS cascade (with claim text).
    pub affected_assertions: Vec<crate::domain::AffectedAssertion>,
    /// Subgraph entities that have no active assertions.
    pub blind_entities: Vec<String>,
    pub boundary_entities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contradiction {
    pub new_claim: String,
    pub existing_claim: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitReport {
    pub ops_applied: usize,
    pub ops_skipped: usize,
    pub details: Vec<String>,
}
