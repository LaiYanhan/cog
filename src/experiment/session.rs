use serde::{Deserialize, Serialize};
use uuid::Uuid;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};

use crate::domain::*;
use crate::repo::Repository;
use crate::space::{CascadeEngine, SemanticSpace, StructureSpace};

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
    /// UUID of the focus entity (resolved from entity_focus at start time).
    #[serde(default)]
    pub entity_focus_id: String,
    pub created_at: DateTime<Utc>,
    pub status: ExperimentStatus,
    pub ops: Vec<ExperimentOp>,
    /// Structural sub-space snapshot (entities + relations).
    #[serde(default)]
    pub structure: StructureSpace,
    /// Semantic sub-space snapshot (assertions + evidence).
    #[serde(default)]
    pub semantic: SemanticSpace,
    /// Number of entities on the boundary of the loaded subgraph.
    pub boundary_count: usize,
    // ── Evaluation results (filled after evaluate()) ──
    pub risk_score: Option<f64>,
    pub affected: Vec<AffectedAssertion>,
    pub contradictions: Vec<Contradiction>,
    /// Whether the experiment has been explicitly saved.
    /// Unsaved experiments are in-progress drafts; `save` marks them as persisted.
    #[serde(default)]
    pub saved: bool,
}

impl Experiment {
    // ── Construction ──────────────────────────────────────────────────────

    /// Start a new experiment by loading a subgraph around `entity_name`.
    pub fn start(
        repo: &dyn Repository,
        entity_name: &str,
        description: String,
        max_nodes: usize,
    ) -> Result<Self> {
        // Resolve the focus entity (exact or fuzzy suffix match).
        let focus = repo.resolve_entity(entity_name)?;
        // Load structural sub-space around the focus (BFS, no depth limit, cap by max_nodes).
        let structure = StructureSpace::load(repo, &focus, 0, max_nodes)?;
        let boundary_count = structure.boundary_count;

        // Load semantic sub-space for the focus entity
        let semantic = SemanticSpace::load(repo, &focus.id)?;
        Ok(Experiment {
            id: Uuid::new_v4().to_string(),
            description,
            entity_focus: entity_name.to_string(),
            entity_focus_id: focus.id.clone(),
            created_at: Utc::now(),
            status: ExperimentStatus::Open,
            ops: Vec::new(),
            structure,
            semantic,
            boundary_count,
            risk_score: None,
            affected: Vec::new(),
            contradictions: Vec::new(),
            saved: false,
        })
    }
    // ── Open state ────────────────────────────────────────────────────────

    /// Add a hypothetical operation. Does NOT touch the real repository.
    pub fn hypothesize(&mut self, op: ExperimentOp) {
        self.ops.push(op);
    }

    /// Evaluate all hypothetical ops: simulate cascades and detect contradictions.
    /// Pure computation — does not mutate the experiment.
    pub fn evaluate(&self) -> Result<ExperimentReport> {
        if self.status != ExperimentStatus::Open && self.status != ExperimentStatus::Evaluated {
            bail!("experiment must be in Open or Evaluated state to evaluate");
        }

        let mut cascade_affected = Vec::new();
        let mut contradictions = Vec::new();

        for op in &self.ops {
            match op {
                ExperimentOp::Retraction {
                    assertion_id,
                    reason: _reason,
                } => {
                    if let Some(cascade) = self.semantic.simulate_retract(assertion_id) {
                        cascade_affected.extend(cascade.affected);
                    }
                }
                ExperimentOp::Assertion {
                    entity_name,
                    kind,
                    claim,
                    ..
                } => {
                    for node in self.semantic.assertions.values() {
                        let a = &node.assertion;
                        let entity_match = self.structure.entities.values().any(|en| {
                            en.entity.qualified_name == *entity_name && en.entity.id == a.entity_id
                        });

                        if entity_match && a.kind == *kind && a.is_active() && a.claim != *claim {
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
                ExperimentOp::Delete { entity_name } => {
                    for node in self.semantic.assertions.values() {
                        let en = self
                            .structure
                            .entities
                            .values()
                            .find(|en| en.entity.qualified_name == *entity_name);
                        if let Some(en) = en
                            && node.assertion.entity_id == en.entity.id
                        {
                            contradictions.push(Contradiction {
                                new_claim: format!("delete entity {entity_name}"),
                                existing_claim: node.assertion.claim.clone(),
                                reason: "assertion would be orphaned by entity deletion"
                                    .to_string(),
                            });
                        }
                    }
                }
                ExperimentOp::Relation { .. } => {}
            }
        }

        // Detect blind entities: entities in the subgraph with no active assertions
        let blind_entities: Vec<String> = self
            .structure
            .entities
            .values()
            .filter(|en| {
                !self
                    .semantic
                    .assertions
                    .values()
                    .any(|n| n.assertion.entity_id == en.entity.id && n.assertion.is_active())
            })
            .map(|en| en.entity.qualified_name.clone())
            .collect();

        let affected_assertions: Vec<AffectedAssertion> = cascade_affected.clone();

        let risk =
            self.semantic
                .assess_risk(&self.entity_focus_id, &self.entity_focus, &self.structure);

        Ok(ExperimentReport {
            experiment_id: self.id.clone(),
            description: self.description.clone(),
            entity_focus: self.entity_focus.clone(),
            ops_count: self.ops.len(),
            risk_score: risk.risk_score,
            affected_count: cascade_affected.len() + contradictions.len(),
            cascade_count: cascade_affected.len(),
            contradictions,
            affected_assertions,
            blind_entities,
            boundary_count: self.boundary_count,
        })
    }

    /// Mark the experiment as evaluated (call after `evaluate()`).
    pub fn mark_evaluated(&mut self) -> Result<()> {
        match self.status {
            ExperimentStatus::Evaluated => return Ok(()),
            ExperimentStatus::Open => {}
            _ => bail!("experiment must be in Open state to mark as evaluated"),
        }
        self.status = ExperimentStatus::Evaluated;
        Ok(())
    }

    /// Mark the experiment as explicitly saved (checkpoint).
    pub fn mark_saved(&mut self) {
        self.saved = true;
    }

    /// Replay all ops to the real repository. Returns a commit report.
    pub fn commit(mut self, repo: &dyn Repository) -> Result<CommitReport> {
        if self.status != ExperimentStatus::Evaluated {
            bail!("experiment must be in Evaluated state to commit");
        }

        let mut applied = 0usize;
        let mut skipped = 0usize;
        let mut details = Vec::new();

        for op in &self.ops {
            match op {
                ExperimentOp::Assertion {
                    entity_name,
                    kind,
                    claim,
                    grounds,
                    depends_on,
                } => {
                    // Resolve entity name — suffix matching like cog assert uses
                    match repo.resolve_entity(entity_name) {
                        Ok(entity) => {
                            let dep = depends_on.as_deref();
                            repo.create_assertion(&entity.id, *kind, claim, grounds, dep)?;
                            applied += 1;
                            details.push(format!("asserted on {entity_name}: {claim}"));
                        }
                        Err(_) => {
                            skipped += 1;
                            details.push(format!(
                                "skipped assertion: entity not found: {entity_name}"
                            ));
                        }
                    }
                }
                ExperimentOp::Retraction {
                    assertion_id,
                    reason,
                } => {
                    // Resolve short id if needed
                    match repo.resolve_assertion_id(assertion_id) {
                        Ok(resolved) => {
                            // Use cascade engine so dependents are properly notified
                            let cascade = CascadeEngine::retract(repo, &resolved, reason)?;
                            applied += 1;
                            details.push(format!(
                                "retracted {assertion_id}: {reason} (cascade: {} affected)",
                                cascade.affected.len()
                            ));
                        }
                        Err(_) => {
                            skipped += 1;
                            details.push(format!(
                                "skipped retraction: assertion not found: {assertion_id}"
                            ));
                        }
                    }
                }
                ExperimentOp::Delete { entity_name } => match repo.delete_entity(entity_name)? {
                    true => {
                        applied += 1;
                        details.push(format!("deleted entity: {entity_name}"));
                    }
                    false => {
                        skipped += 1;
                        details.push(format!("skipped delete: entity not found: {entity_name}"));
                    }
                },
                ExperimentOp::Relation {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::SqliteRepository;

    /// Regression: experiment commit must resolve short entity names via suffix
    /// matching, not exact match. Previously used get_entity_by_name which
    /// silently skipped assertions for names like "_get_callable_signature"
    /// when the stored name was "wat::inspection::_get_callable_signature".
    #[test]
    fn commit_resolves_short_entity_name() -> Result<()> {
        let repo = SqliteRepository::open_in_memory()?;

        // Create entity with a fully qualified name (as tree-sitter would)
        repo.upsert_entity(
            "wat::inspection::inspection::_get_callable_signature",
            EntityKind::Function,
            EntityOrigin::Scan,
        )?;

        // Build a minimal experiment in Evaluated state
        let mut exp = Experiment {
            id: Uuid::new_v4().to_string(),
            description: "test".into(),
            entity_focus: "_get_callable_signature".into(),
            entity_focus_id: String::new(),
            created_at: Utc::now(),
            status: ExperimentStatus::Evaluated,
            ops: vec![ExperimentOp::Assertion {
                entity_name: "_get_callable_signature".into(),
                kind: AssertionKind::Contract,
                claim: "does X".into(),
                grounds: "code:_get_callable_signature".into(),
                depends_on: None,
            }],
            structure: StructureSpace::default(),
            semantic: SemanticSpace::default(),
            boundary_count: 0,
            risk_score: None,
            affected: vec![],
            contradictions: vec![],
            saved: false,
        };
        exp.mark_saved();

        let report = exp.commit(&repo)?;

        assert_eq!(report.ops_applied, 1, "should apply, not skip");
        assert_eq!(
            report.ops_skipped, 0,
            "expected zero skips, got: {:?}",
            report.details
        );

        // Verify the assertion was actually created on the correct entity
        let entity = repo.resolve_entity("_get_callable_signature")?;
        let assertions = repo.get_assertions_for_entity(&entity.id)?;
        assert_eq!(assertions.len(), 1);
        assert_eq!(assertions[0].claim, "does X");

        Ok(())
    }

    /// Verify that truly unknown entities still get skipped.
    #[test]
    fn commit_skips_unknown_entity() -> Result<()> {
        let repo = SqliteRepository::open_in_memory()?;

        let mut exp = Experiment {
            id: Uuid::new_v4().to_string(),
            description: "test".into(),
            entity_focus: "nonexistent".into(),
            entity_focus_id: String::new(),
            created_at: Utc::now(),
            status: ExperimentStatus::Evaluated,
            ops: vec![ExperimentOp::Assertion {
                entity_name: "no_such_entity".into(),
                kind: AssertionKind::Contract,
                claim: "never stored".into(),
                grounds: "note:test".into(),
                depends_on: None,
            }],
            structure: StructureSpace::default(),
            semantic: SemanticSpace::default(),
            boundary_count: 0,
            risk_score: None,
            affected: vec![],
            contradictions: vec![],
            saved: false,
        };
        exp.mark_saved();

        let report = exp.commit(&repo)?;

        assert_eq!(
            report.ops_applied, 0,
            "unknown entity should not be applied"
        );
        assert_eq!(report.ops_skipped, 1);
        assert!(report.details[0].contains("entity not found"));

        Ok(())
    }
}
