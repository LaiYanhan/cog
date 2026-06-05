use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::*;
use crate::repo::Repository;

use super::ops::ExperimentOp;
use super::report::{CommitReport, Contradiction, ExperimentReport};

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperimentStatus {
    Open,
    Evaluated,
    Committed,
    Discarded,
}

// ---------------------------------------------------------------------------
// Experiment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub id: String,
    pub description: String,
    pub entity_focus: String,
    pub status: ExperimentStatus,
    pub ops: Vec<ExperimentOp>,
    /// In-memory snapshot of entities relevant to the experiment, keyed by id.
    pub entities: HashMap<String, Entity>,
    /// In-memory snapshot of assertions relevant to the experiment, keyed by id.
    pub assertions: HashMap<String, Assertion>,
    /// In-memory assertion relations (for dependency tracing).
    #[serde(default)]
    pub assertion_relations: Vec<AssertionRelation>,
    /// Entities on the boundary of the loaded subgraph (partial data).
    pub boundary_entities: Vec<String>,
    // ── Evaluation results (filled after evaluate()) ──
    pub risk_score: Option<f64>,
    pub affected: Vec<AffectedAssertion>,
    pub contradictions: Vec<Contradiction>,
}

impl Experiment {
    // ── Construction ──────────────────────────────────────────────────────

    /// Start a new experiment by loading a subgraph around `entity_name`.
    ///
    /// BFS from the focus entity, expanding via entity relations, up to
    /// `max_nodes` entities (default 500).
    pub fn start(
        repo: &dyn Repository,
        entity_name: &str,
        description: String,
        max_nodes: usize,
    ) -> Result<Self> {
        let focus = repo
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow::anyhow!("entity not found: {entity_name}"))?;

        // BFS to collect entity IDs
        let mut visited: HashSet<String> = HashSet::new();
        let mut frontier: Vec<String> = vec![focus.id.clone()];
        let mut boundary = Vec::new();

        while !frontier.is_empty() && visited.len() < max_nodes {
            let next_id = frontier.remove(0);
            if visited.contains(&next_id) {
                continue;
            }
            visited.insert(next_id.clone());

            let related = repo.get_related_entities(&next_id)?;
            for rel in related {
                let neighbor_id = rel.entity.id.clone();
                if !visited.contains(&neighbor_id) {
                    if visited.len() + frontier.len() < max_nodes {
                        if !frontier.contains(&neighbor_id) {
                            frontier.push(neighbor_id);
                        }
                    } else {
                        boundary.push(rel.entity.qualified_name.clone());
                    }
                }
            }
        }

        // Load full entities and assertions for visited set
        let mut entities = HashMap::new();
        let mut assertions = HashMap::new();
        entities.insert(focus.id.clone(), focus);

        for eid in &visited {
            if let Some(entity) = repo.get_entity(eid)? {
                entities.insert(entity.id.clone(), entity);
            }
        }

        // Batch-load assertions for all visited entities
        let entity_ids: Vec<String> = visited.iter().cloned().collect();
        let all_assertions = repo.get_assertions_for_entities(&entity_ids)?;
        for a in all_assertions {
            assertions.insert(a.id.clone(), a);
        }

        // Load assertion relations for tracing dependents during evaluate
        let assertion_relations = repo.list_assertion_relations()?;

        Ok(Experiment {
            id: Uuid::new_v4().to_string(),
            description,
            entity_focus: entity_name.to_string(),
            status: ExperimentStatus::Open,
            ops: Vec::new(),
            entities,
            assertions,
            assertion_relations,
            boundary_entities: boundary,
            risk_score: None,
            affected: Vec::new(),
            contradictions: Vec::new(),
        })
    }

    // ── Open state ────────────────────────────────────────────────────────

    /// Add a hypothetical operation. Does NOT touch the real repository.
    pub fn hypothesize(&mut self, op: ExperimentOp) {
        self.ops.push(op);
    }

    /// Evaluate all hypothetical ops: simulate cascades and detect contradictions.
    pub fn evaluate(&mut self) -> Result<()> {
        if self.status != ExperimentStatus::Open {
            bail!("experiment must be in Open state to evaluate");
        }

        let mut affected = Vec::new();
        let mut contradictions = Vec::new();

        for op in &self.ops {
            match op {
                ExperimentOp::HypotheticalRetraction {
                    assertion_id,
                    reason: _reason,
                } => {
                    // Trace dependents in-memory via assertion_relations
                    let cascade =
                        Self::trace_dependents(assertion_id, &self.assertions, &self.assertion_relations);
                    affected.extend(cascade);
                }
                ExperimentOp::HypotheticalAssertion {
                    entity_name,
                    kind,
                    claim,
                    ..
                } => {
                    // Find entity by name in snapshot
                    let entity_id = self
                        .entities
                        .values()
                        .find(|e| e.qualified_name == *entity_name)
                        .map(|e| e.id.as_str());

                    if let Some(eid) = entity_id {
                        // Check for contradictions with existing active assertions
                        for a in self.assertions.values() {
                            if a.entity_id == eid
                                && a.kind == *kind
                                && a.is_active()
                                && a.claim != *claim
                            {
                                contradictions.push(Contradiction {
                                    new_claim: claim.clone(),
                                    existing_claim: a.claim.clone(),
                                    reason: format!(
                                        "same entity and kind, different claim (existing: {})",
                                        a.short_id()
                                    ),
                                });
                            }
                        }
                    }
                }
                ExperimentOp::HypotheticalRelation { .. } => {
                    // No cascade/contradiction implications for relations
                }
            }
        }

        let total = self.assertions.len().max(1);
        let affected_count = affected.len();
        let risk_score = affected_count as f64 / total as f64;

        self.risk_score = Some(risk_score);
        self.affected = affected;
        self.contradictions = contradictions;
        self.status = ExperimentStatus::Evaluated;

        Ok(())
    }

    // ── Evaluated state ───────────────────────────────────────────────────

    /// Generate a report from the evaluation results.
    pub fn report(&self) -> ExperimentReport {
        let risk = self.risk_score.unwrap_or(0.0);
        ExperimentReport {
            experiment_id: self.id.clone(),
            description: self.description.clone(),
            entity_focus: self.entity_focus.clone(),
            ops_count: self.ops.len(),
            risk_score: risk,
            affected_count: self.affected.len(),
            contradictions: self.contradictions.clone(),
            boundary_entities: self.boundary_entities.clone(),
        }
    }

    /// Replay all ops to the real repository. Returns a commit report.
    pub fn commit(&mut self, repo: &dyn Repository) -> Result<CommitReport> {
        if self.status != ExperimentStatus::Evaluated {
            bail!("experiment must be in Evaluated state to commit");
        }

        let mut applied = 0usize;
        let mut skipped = 0usize;
        let mut details = Vec::new();

        for op in &self.ops {
            match op {
                ExperimentOp::HypotheticalAssertion {
                    entity_name,
                    kind,
                    claim,
                    grounds,
                    depends_on,
                } => {
                    // Resolve entity name to id
                    match repo.get_entity_by_name(entity_name)? {
                        Some(entity) => {
                            let dep = depends_on.as_deref();
                            repo.create_assertion(
                                &entity.id,
                                *kind,
                                claim,
                                grounds,
                                dep,
                            )?;
                            applied += 1;
                            details.push(format!("asserted on {entity_name}: {claim}"));
                        }
                        None => {
                            skipped += 1;
                            details.push(format!("skipped assertion: entity not found: {entity_name}"));
                        }
                    }
                }
                ExperimentOp::HypotheticalRetraction {
                    assertion_id,
                    reason,
                } => {
                    // Resolve short id if needed
                    match repo.resolve_assertion_id(assertion_id) {
                        Ok(resolved) => {
                            repo.retract_assertion(&resolved, reason)?;
                            applied += 1;
                            details.push(format!("retracted {assertion_id}: {reason}"));
                        }
                        Err(_) => {
                            skipped += 1;
                            details
                                .push(format!("skipped retraction: assertion not found: {assertion_id}"));
                        }
                    }
                }
                ExperimentOp::HypotheticalRelation {
                    from_entity,
                    to_entity,
                    kind,
                } => {
                    match (
                        repo.get_entity_by_name(from_entity)?,
                        repo.get_entity_by_name(to_entity)?,
                    ) {
                        (Some(from), Some(to)) => {
                            repo.add_entity_relation(&from.id, &to.id, *kind)?;
                            applied += 1;
                            details.push(format!(
                                "added relation: {from_entity} -> {to_entity} ({kind})"
                            ));
                        }
                        _ => {
                            skipped += 1;
                            details.push(format!(
                                "skipped relation: entity not found: {from_entity} or {to_entity}"
                            ));
                        }
                    }
                }
            }
        }

        self.status = ExperimentStatus::Committed;
        Ok(CommitReport {
            ops_applied: applied,
            ops_skipped: skipped,
            details,
        })
    }

    /// Discard the experiment. Consumes self without side effects.
    pub fn discard(mut self) {
        self.status = ExperimentStatus::Discarded;
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Trace dependents of a retracted assertion using in-memory data.
    fn trace_dependents(
        assertion_id: &str,
        assertions: &HashMap<String, Assertion>,
        relations: &[AssertionRelation],
    ) -> Vec<AffectedAssertion> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![assertion_id.to_string()];

        while let Some(current_id) = queue.pop() {
            if !visited.insert(current_id.clone()) {
                continue;
            }
            // Find assertions that depend on the current one
            for rel in relations {
                if rel.to_assertion == current_id && !visited.contains(&rel.from_assertion) {
                    queue.push(rel.from_assertion.clone());
                }
            }
            // Record as affected (skip the root retraction itself)
            if current_id != assertion_id {
                if let Some(a) = assertions.get(&current_id) {
                    result.push(AffectedAssertion {
                        assertion: a.clone(),
                        cascade_reason: CascadeReason::GroundWeakened,
                    });
                }
            }
        }

        result
    }
}
