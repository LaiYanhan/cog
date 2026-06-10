use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::RiskAssessment;
use crate::domain::{
    AffectedAssertion, Assertion, AssertionStatus, CascadeReason, CascadeReport, Evidence,
};
use crate::repo::Repository;
use crate::space::StructureSpace;
use anyhow::Result;

// ---------------------------------------------------------------------------
// Node types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionNode {
    pub assertion: Assertion,
    pub evidences: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceNode {
    pub evidence: Evidence,
    pub assertion_id: String,
}

// ---------------------------------------------------------------------------
// SemanticSpace
// ---------------------------------------------------------------------------

/// Read-only view of the semantic sub-space (§2.5.2) — the TMS belief system.
///
/// Loaded from a Repository into memory for offline analysis and simulation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticSpace {
    pub assertions: HashMap<String, AssertionNode>,
    pub evidence: HashMap<String, EvidenceNode>,
    /// (dependent_id, dependency_id) edges.
    pub depends_on: Vec<(String, String)>,
}

impl SemanticSpace {
    // ── Loading ──────────────────────────────────────────────────────────

    /// Load assertions and their evidence for a given entity (and optionally
    /// its related entities).  Expands one hop to include assertions on
    /// entities directly related to `entity_id`.
    pub fn load(repo: &dyn Repository, entity_id: &str) -> Result<Self> {
        let mut assertions_map: HashMap<String, AssertionNode> = HashMap::new();
        let mut evidence_map: HashMap<String, EvidenceNode> = HashMap::new();
        let mut deps: Vec<(String, String)> = Vec::new();
        let mut seen_entities: HashSet<String> = HashSet::new();

        // Mark focus entity as seen
        seen_entities.insert(entity_id.to_string());

        // Also load assertions for related entities (one hop)
        let related = repo.get_related_entities(entity_id)?;
        let mut entity_ids: Vec<String> = vec![entity_id.to_string()];
        for r in &related {
            if seen_entities.insert(r.entity.id.clone()) {
                entity_ids.push(r.entity.id.clone());
            }
        }

        // Batch-load assertions for all collected entity ids
        let all_assertions = repo.get_assertions_for_entities(&entity_ids)?;

        for assertion in all_assertions {
            let evidences = repo.get_evidence_for_assertion(&assertion.id)?;
            for ev in &evidences {
                evidence_map.insert(
                    ev.id.clone(),
                    EvidenceNode {
                        evidence: ev.clone(),
                        assertion_id: assertion.id.clone(),
                    },
                );
            }
            assertions_map.insert(
                assertion.id.clone(),
                AssertionNode {
                    assertion,
                    evidences,
                },
            );
        }

        // Load assertion relations (depends_on edges)
        let all_relations = repo.list_assertion_relations()?;
        for rel in all_relations {
            // Only keep edges where both endpoints are in our loaded assertions
            if assertions_map.contains_key(&rel.from_assertion)
                && assertions_map.contains_key(&rel.to_assertion)
            {
                deps.push((rel.from_assertion.clone(), rel.to_assertion.clone()));
            }
        }

        Ok(Self {
            assertions: assertions_map,
            evidence: evidence_map,
            depends_on: deps,
        })
    }

    // ── Simulation ───────────────────────────────────────────────────────

    /// Simulate retracting `assertion_id`: BFS along reverse depends_on
    /// edges, marking transitive dependents as Uncertain.  Returns a
    /// `CascadeReport` describing what *would* happen without touching the
    /// real database.
    pub fn simulate_retract(&self, assertion_id: &str) -> Option<CascadeReport> {
        let retracted = self.assertions.get(assertion_id)?.assertion.clone();

        let mut affected: Vec<AffectedAssertion> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        queue.push_back(assertion_id.to_string());

        let mut reverse_deps: HashMap<&str, Vec<&str>> = HashMap::new();
        for (from, to) in &self.depends_on {
            reverse_deps
                .entry(to.as_str())
                .or_default()
                .push(from.as_str());
        }

        while let Some(current_id) = queue.pop_front() {
            if !visited.insert(current_id.clone()) {
                continue;
            }

            if let Some(dependents) = reverse_deps.get(current_id.as_str()) {
                for dep_id in dependents {
                    if visited.contains(*dep_id) {
                        continue;
                    }
                    if let Some(node) = self.assertions.get(*dep_id) {
                        if node.assertion.status == AssertionStatus::Retracted {
                            continue;
                        }
                        let reason = if current_id == assertion_id {
                            CascadeReason::MarkedUncertain
                        } else {
                            CascadeReason::GroundWeakened
                        };
                        affected.push(AffectedAssertion {
                            assertion: node.assertion.clone(),
                            cascade_reason: reason,
                        });
                        queue.push_back(dep_id.to_string());
                    }
                }
            }
        }

        Some(CascadeReport {
            retracted,
            affected,
        })
    }

    /// Assess the risk of modifying an entity, producing a `RiskAssessment`.
    ///
    /// Considers: fan-in from the structure space, active assertions count,
    /// fragility (invariant/fragility assertions), and downstream dependencies.
    pub fn assess_risk(&self, entity_id: &str, structure: &StructureSpace) -> RiskAssessment {
        // Count assertions for this entity
        let entity_assertions: Vec<&AssertionNode> = self
            .assertions
            .values()
            .filter(|n| n.assertion.entity_id == entity_id)
            .collect();

        let active_count = entity_assertions
            .iter()
            .filter(|n| n.assertion.status == AssertionStatus::Active)
            .count();

        let fragile_count = entity_assertions
            .iter()
            .filter(|n| {
                n.assertion.kind.to_string() == "fragility"
                    || n.assertion.kind.to_string() == "invariant"
            })
            .count();

        // Count downstream entities from structure space.
        // Uses the same edge-filtered logic as the impact BFS:
        // only Calls + Uses reverse edges (who depends on this entity),
        // not Contains (structural) edges.
        let downstream: Vec<&crate::space::structure::EntityNode> = {
            let mut all = Vec::new();
            for kind in [
                crate::domain::EntityRelationKind::Calls,
                crate::domain::EntityRelationKind::Uses,
            ] {
                all.extend(structure.dependents_of_kind(entity_id, kind));
            }
            all.sort_by_key(|n| &n.entity.qualified_name);
            all.dedup_by_key(|n| &n.entity.id);
            all
        };
        let downstream_count = downstream.len();

        // Compute downstream coverage: ratio of downstream entities that have
        // at least one active assertion, and count unmodeled downstream entities.
        let (downstream_coverage, unmodeled_downstream) = {
            let downstream_with_assertions = downstream
                .iter()
                .filter(|dep| {
                    self.assertions.values().any(|n| {
                        n.assertion.entity_id == dep.entity.id
                            && n.assertion.status == AssertionStatus::Active
                    })
                })
                .count();
            let denom = downstream_count.max(1) as f64;
            (
                downstream_with_assertions as f64 / denom,
                downstream_count - downstream_with_assertions,
            )
        };

        // Risk heuristic: high downstream + many active assertions → high risk
        // Public entities add extra exposure
        let is_public = structure
            .entities
            .get(entity_id)
            .map(|en| en.entity.metrics.visibility.is_public())
            .unwrap_or(false);

        let base_risk = if downstream_count >= 10 && active_count >= 5 {
            0.9
        } else if downstream_count >= 5 || active_count >= 3 {
            0.6
        } else if downstream_count > 0 || active_count > 0 {
            0.3
        } else {
            0.1
        };

        // Public visibility increases exposure
        let risk_score = if is_public {
            f64::min(base_risk + 0.1, 1.0)
        } else {
            base_risk
        };

        // When an entity is well-asserted (>=3 active) but has no downstream
        // dependencies, floor the risk score at 0.30 — assertions alone still
        // signal a non-trivial maintenance surface.
        let risk_score = if active_count >= 3 && downstream_count == 0 {
            risk_score.max(0.30)
        } else {
            risk_score
        };

        // Build summary
        let summary = if risk_score >= 0.8 {
            format!(
                "High risk: {downstream_count} downstream entities, {active_count} active assertions"
            )
        } else if risk_score >= 0.5 {
            format!("Medium risk: {downstream_count} downstream, {active_count} assertions")
        } else {
            "Low risk".to_string()
        };

        RiskAssessment {
            entity_name: entity_id.to_string(),
            risk_score,
            downstream_count,
            active_assertions: active_count,
            fragile_assertions: fragile_count,
            summary,
            downstream_coverage,
            unmodeled_downstream,
        }
    }
}
