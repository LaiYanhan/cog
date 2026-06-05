use std::collections::HashSet;

use anyhow::{Result, anyhow};

use crate::domain::*;
use crate::repo::Repository;

pub struct TraceEngine;

impl TraceEngine {
    pub fn trace(repo: &dyn Repository, entity_name: &str) -> Result<TraceTree> {
        let entity = repo
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow!("entity not found: {entity_name}"))?;

        let assertions = repo
            .get_assertions_for_entity(&entity.id)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .map(|assertion| build_trace_assertion(repo, assertion, &mut HashSet::new()))
            .collect::<Result<Vec<_>>>()?;

        let related_entities = repo.get_related_entities(&entity.id)?;

        Ok(TraceTree {
            entity,
            assertions,
            related_entities,
        })
    }
}

fn build_trace_assertion(
    repo: &dyn Repository,
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

    let evidences = repo.get_evidence_for_assertion(&assertion.id)?;
    let dependencies = repo
        .get_dependencies(&assertion.id)?
        .into_iter()
        .map(|dep| build_trace_assertion(repo, dep, seen))
        .collect::<Result<Vec<_>>>()?;
    seen.remove(&assertion.id);

    Ok(TraceAssertion {
        assertion,
        evidences,
        dependencies,
    })
}
