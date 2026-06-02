use std::collections::{HashSet, VecDeque};

use anyhow::{Result, anyhow, bail};

use crate::model::{
    Assertion, AssertionStatus, Changelog, ChangelogAction, Entity, Evidence, RelatedEntity, Store,
};

#[derive(Debug, Clone)]
pub struct CascadeResult {
    pub retracted: Assertion,
    pub affected: Vec<AffectedAssertion>,
}

#[derive(Debug, Clone)]
pub struct AffectedAssertion {
    pub assertion: Assertion,
    pub cascade_reason: CascadeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CascadeReason {
    MarkedUncertain,
    GroundWeakened,
}

impl CascadeResult {
    pub fn retract(store: &Store, assertion_id: &str, reason: &str) -> Result<Self> {
        let current = store
            .get_assertion(assertion_id)?
            .ok_or_else(|| anyhow!("assertion not found: {assertion_id}"))?;
        if current.status == AssertionStatus::Retracted {
            bail!("assertion already retracted: {assertion_id}");
        }

        store.transaction(|| {
            store.retract_assertion(assertion_id, reason)?;
            Changelog::append(store, ChangelogAction::Retract, assertion_id, reason)?;

            let mut queue = VecDeque::from([assertion_id.to_string()]);
            let mut seen = HashSet::new();
            let mut affected = Vec::new();

            while let Some(current_id) = queue.pop_front() {
                if !seen.insert(current_id.clone()) {
                    continue;
                }

                for dependent in store.get_dependents(&current_id)? {
                    if dependent.status == AssertionStatus::Retracted {
                        continue;
                    }

                    let dependencies = store.get_dependencies(&dependent.id)?;
                    let has_independent_active = dependencies.iter().any(|dep| {
                        dep.id != current_id
                            && dep.status != AssertionStatus::Retracted
                            && dep.status != AssertionStatus::Uncertain
                    });

                    if has_independent_active {
                        affected.push(AffectedAssertion {
                            assertion: dependent,
                            cascade_reason: CascadeReason::GroundWeakened,
                        });
                        continue;
                    }

                    let mut updated = dependent;
                    if updated.status != AssertionStatus::Uncertain {
                        store.update_assertion_status(&updated.id, AssertionStatus::Uncertain)?;
                        updated.status = AssertionStatus::Uncertain;
                    }

                    Changelog::append(
                        store,
                        ChangelogAction::CascadeMark,
                        &updated.id,
                        &format!("marked uncertain due to dependency retraction: {current_id}"),
                    )?;

                    queue.push_back(updated.id.clone());
                    affected.push(AffectedAssertion {
                        assertion: updated,
                        cascade_reason: CascadeReason::MarkedUncertain,
                    });
                }
            }

            let retracted = store
                .get_assertion(assertion_id)?
                .ok_or_else(|| anyhow!("assertion disappeared after retract: {assertion_id}"))?;

            Ok(Self {
                retracted,
                affected,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct ImpactResult {
    pub entity: Entity,
    pub downstream_entities: Vec<Entity>,
    pub affected_assertions: Vec<Assertion>,
}

impl ImpactResult {
    pub fn analyze(store: &Store, entity_name: &str) -> Result<Self> {
        let entity = store
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow!("entity not found: {entity_name}"))?;

        let mut queue = VecDeque::from([entity.id.clone()]);
        let mut seen = HashSet::new();
        let mut entity_ids = Vec::new();

        while let Some(current) = queue.pop_front() {
            if !seen.insert(current.clone()) {
                continue;
            }

            entity_ids.push(current.clone());
            for next in store.get_impact_neighbors(&current)? {
                queue.push_back(next.id);
            }
        }

        let downstream_entities = entity_ids
            .iter()
            .filter(|id| *id != &entity.id)
            .map(|id| store.get_entity(id))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let affected_assertions = store
            .get_assertions_for_entities(&entity_ids)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .collect();

        Ok(Self {
            entity,
            downstream_entities,
            affected_assertions,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TraceResult {
    pub entity: Entity,
    pub assertions: Vec<TraceAssertion>,
    pub related_entities: Vec<RelatedEntity>,
}

#[derive(Debug, Clone)]
pub struct TraceAssertion {
    pub assertion: Assertion,
    pub evidences: Vec<Evidence>,
    pub dependencies: Vec<TraceAssertion>,
}

impl TraceResult {
    pub fn trace(store: &Store, entity_name: &str) -> Result<Self> {
        let entity = store
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow!("entity not found: {entity_name}"))?;

        let assertions = store
            .get_assertions_for_entity(&entity.id)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .map(|assertion| build_trace_assertion(store, assertion, &mut HashSet::new()))
            .collect::<Result<Vec<_>>>()?;

        let related_entities = store.get_related_entities(&entity.id)?;

        Ok(Self {
            entity,
            assertions,
            related_entities,
        })
    }
}

fn build_trace_assertion(
    store: &Store,
    assertion: Assertion,
    seen: &mut HashSet<String>,
) -> Result<TraceAssertion> {
    if !seen.insert(assertion.id.clone()) {
        return Ok(TraceAssertion {
            assertion,
            evidences: Vec::new(),
            dependencies: Vec::new(),
        });
    }

    let evidences = store.get_evidence_for_assertion(&assertion.id)?;
    let dependencies = store
        .get_dependencies(&assertion.id)?
        .into_iter()
        .map(|dependency| build_trace_assertion(store, dependency, seen))
        .collect::<Result<Vec<_>>>()?;
    seen.remove(&assertion.id);

    Ok(TraceAssertion {
        assertion,
        evidences,
        dependencies,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::{CascadeReason, CascadeResult, ImpactResult, TraceResult};
    use crate::model::{AssertionKind, AssertionStatus, EntityKind, EntityRelationKind, Store};

    #[test]
    fn retract_cascades_uncertain_status() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;

        let root = store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "root",
            "code:root",
            None,
        )?;
        let mid = store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "mid",
            "code:mid",
            Some(&root.id),
        )?;
        let leaf = store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "leaf",
            "code:leaf",
            Some(&mid.id),
        )?;

        let result = CascadeResult::retract(&store, &root.id, "invalid")?;
        assert_eq!(result.retracted.status, AssertionStatus::Retracted);
        assert_eq!(result.affected.len(), 2);

        let mid_now = store.get_assertion(&mid.id)?.expect("mid exists");
        let leaf_now = store.get_assertion(&leaf.id)?.expect("leaf exists");
        assert_eq!(mid_now.status, AssertionStatus::Uncertain);
        assert_eq!(leaf_now.status, AssertionStatus::Uncertain);
        Ok(())
    }

    #[test]
    fn retract_keeps_dependent_with_other_support() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;

        let a = store.create_assertion(&entity.id, AssertionKind::Contract, "a", "note:a", None)?;
        let b = store.create_assertion(&entity.id, AssertionKind::Contract, "b", "note:b", None)?;
        let dependent = store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "dependent",
            "note:d",
            Some(&a.id),
        )?;
        store.add_assertion_dependency(&dependent.id, &b.id)?;

        let result = CascadeResult::retract(&store, &a.id, "invalid")?;
        assert!(
            result
                .affected
                .iter()
                .any(|a| a.assertion.id == dependent.id
                    && a.cascade_reason == CascadeReason::GroundWeakened)
        );

        let dependent_now = store
            .get_assertion(&dependent.id)?
            .expect("dependent exists");
        assert_eq!(dependent_now.status, AssertionStatus::Active);
        Ok(())
    }

    #[test]
    fn impact_analysis_follows_multi_hop_relations() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        let a = store.upsert_entity("A", EntityKind::Module)?;
        let b = store.upsert_entity("B", EntityKind::Module)?;
        let c = store.upsert_entity("C", EntityKind::Module)?;
        store.add_entity_relation(&a.id, &b.id, EntityRelationKind::Contains)?;
        store.add_entity_relation(&b.id, &c.id, EntityRelationKind::Contains)?;
        store.create_assertion(&a.id, AssertionKind::Contract, "a", "note:a", None)?;
        store.create_assertion(&b.id, AssertionKind::Contract, "b", "note:b", None)?;
        store.create_assertion(&c.id, AssertionKind::Contract, "c", "note:c", None)?;

        let result = ImpactResult::analyze(&store, "A")?;
        assert_eq!(result.downstream_entities.len(), 2);
        assert_eq!(result.affected_assertions.len(), 3);
        Ok(())
    }

    #[test]
    fn trace_builds_dependency_tree() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;

        let root = store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "root",
            "note:root",
            None,
        )?;
        let child = store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "child",
            "note:child",
            Some(&root.id),
        )?;

        let trace = TraceResult::trace(&store, "auth::login")?;
        assert_eq!(trace.assertions.len(), 2);
        let child_trace = trace
            .assertions
            .iter()
            .find(|n| n.assertion.id == child.id)
            .expect("child trace exists");
        assert_eq!(child_trace.dependencies.len(), 1);
        assert_eq!(child_trace.dependencies[0].assertion.id, root.id);
        Ok(())
    }
}
