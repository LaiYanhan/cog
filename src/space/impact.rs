use anyhow::{Result, anyhow};

use crate::domain::*;
use crate::repo::Repository;
use crate::space::{SemanticSpace, StructureSpace};
pub struct ImpactEngine;

impl ImpactEngine {
    /// Analyze downstream impact of modifying an entity.
    ///
    /// Loads the structure sub-space around the entity, then performs
    /// BFS in pure memory to find all reachable downstream entities
    /// and their associated assertions.
    pub fn analyze(repo: &dyn Repository, entity_name: &str) -> Result<ImpactCard> {
        let entity = repo
            .get_entity_by_name(entity_name)?
            .ok_or_else(|| anyhow!("entity not found: {entity_name}"))?;

        // Load structure space — expand wide (depth 0 = unlimited, cap at 500 nodes)
        let structure = StructureSpace::load(repo, entity_name, 0, 500)?;

        // BFS in memory to collect all downstream entity IDs
        let downstream_ids: Vec<String> = {
            let mut ids = Vec::new();
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back(entity.id.clone());

            while let Some(current_id) = queue.pop_front() {
                if !visited.insert(current_id.clone()) {
                    continue;
                }
                // Skip the root entity itself
                if current_id != entity.id {
                    ids.push(current_id.clone());
                }
                // Expand: entities that depend on current (dependents = incoming edges)
                for node in structure.dependents_of(&current_id) {
                    if !visited.contains(&node.entity.id) {
                        queue.push_back(node.entity.id.clone());
                    }
                }
                // Also expand via dependencies (outgoing edges)
                for node in structure.dependencies_of(&current_id) {
                    if !visited.contains(&node.entity.id) {
                        queue.push_back(node.entity.id.clone());
                    }
                }
            }
            ids
        };

        // Resolve downstream entities
        let downstream_entities: Vec<Entity> = downstream_ids
            .iter()
            .filter_map(|id| repo.get_entity(id).ok().flatten())
            .collect();

        // Collect all entity IDs for the root + downstream
        let mut all_entity_ids = vec![entity.id.clone()];
        all_entity_ids.extend(downstream_ids);

        let affected_assertions = repo
            .get_assertions_for_entities(&all_entity_ids)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .collect();

        // Risk assessment: load semantic space and evaluate
        let risk_assessment = SemanticSpace::load(repo, &entity.id)
            .ok()
            .map(|semantic| semantic.assess_risk(&entity.id, &structure));

        Ok(ImpactCard {
            entity,
            downstream_entities,
            affected_assertions,
            risk_assessment,
        })
    }
}
