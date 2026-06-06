use std::collections::HashSet;

use anyhow::{Result, anyhow};

use crate::domain::*;
use crate::repo::Repository;
use crate::space::SemanticSpace;

pub struct TraceEngine;

impl TraceEngine {
    /// Build a dependency trace tree for an entity.
    ///
    /// Loads the semantic sub-space around the entity, then performs
    /// DFS in pure memory to trace assertion dependency chains.
    pub fn trace(repo: &dyn Repository, entity_name: &str) -> Result<TraceTree> {
        let entity = repo
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow!("entity not found: {entity_name}"))?;

        // Load semantic space for this entity
        let semantic = SemanticSpace::load(repo, &entity.id)?;

        // Find active assertions for this entity and build their trace trees
        let assertions: Vec<TraceAssertion> = semantic
            .assertions
            .values()
            .filter(|n| n.assertion.entity_id == entity.id)
            .filter(|n| n.assertion.status == AssertionStatus::Active)
            .filter_map(|n| build_trace(&semantic, &n.assertion.id, &mut HashSet::new()))
            .collect();

        let related_entities = repo.get_related_entities(&entity.id)?;

        Ok(TraceTree {
            entity,
            assertions,
            related_entities,
        })
    }
}

/// DFS along depends_on edges (assertion → its dependencies).
/// Returns `None` if the assertion isn't in the semantic space.
fn build_trace(
    space: &SemanticSpace,
    assertion_id: &str,
    visited: &mut HashSet<String>,
) -> Option<TraceAssertion> {
    // Guard: avoid infinite recursion on cycles
    if !visited.insert(assertion_id.to_string()) {
        return None;
    }

    let node = space.assertions.get(assertion_id)?;

    // Find dependencies: (from, to) where from == assertion_id
    let dependencies: Vec<TraceAssertion> = space
        .depends_on
        .iter()
        .filter(|(from, _)| from == assertion_id)
        .filter_map(|(_, to)| build_trace(space, to, visited))
        .collect();

    visited.remove(assertion_id);

    Some(TraceAssertion {
        assertion: node.assertion.clone(),
        evidences: node.evidences.clone(),
        dependencies,
    })
}
