use serde::{Deserialize, Serialize};

/// Risk assessment for modifying an entity.
///
/// Produced by [`SemanticSpace::assess_risk`](super::SemanticSpace::assess_risk),
/// considering fan-in, active assertions, fragility assertions, and
/// downstream dependencies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskAssessment {
    pub entity_name: String,
    pub risk_score: f64,
    pub downstream_count: usize,
    pub active_assertions: usize,
    pub fragile_assertions: usize,
    pub summary: String,
}
