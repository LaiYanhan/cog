use anyhow::Result;

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
    ///
    /// Only follows `Calls` and `Uses` edges in reverse direction
    /// (who depends on this entity).  `Contains` edges are structural,
    /// not dependency, so they are excluded from the traversal.
    pub fn analyze(repo: &dyn Repository, entity_name: &str) -> Result<ImpactCard> {
        let entity = repo.resolve_entity(entity_name)?;

        // Load structure space — expand wide (depth 0 = unlimited, cap at 500 nodes)
        let structure = StructureSpace::load(repo, &entity, 0, 500)?;

        // BFS: only follow Calls + Uses reverse edges (who depends on me).
        // Contains edges are structural, not dependency, so they're excluded.
        let (direct_ids, indirect_ids): (Vec<String>, Vec<String>) = {
            let mut direct = Vec::new();
            let mut indirect = Vec::new();
            let mut visited = std::collections::HashSet::new();
            let mut queue = std::collections::VecDeque::new();
            queue.push_back((entity.id.clone(), 0usize));

            while let Some((current_id, hop)) = queue.pop_front() {
                if !visited.insert(current_id.clone()) {
                    continue;
                }
                if current_id != entity.id {
                    if hop == 1 {
                        direct.push(current_id.clone());
                    } else {
                        indirect.push(current_id.clone());
                    }
                }
                // Only expand via dependency edges (reverse direction)
                for kind in [EntityRelationKind::Calls, EntityRelationKind::Uses] {
                    for node in structure.dependents_of_kind(&current_id, kind) {
                        if !visited.contains(&node.entity.id) {
                            queue.push_back((node.entity.id.clone(), hop + 1));
                        }
                    }
                }
            }
            (direct, indirect)
        };

        let downstream_ids: Vec<String> = direct_ids
            .iter()
            .chain(indirect_ids.iter())
            .cloned()
            .collect();

        // Resolve downstream entities
        let downstream_entities: Vec<Entity> = downstream_ids
            .iter()
            .filter_map(|id| repo.get_entity(id).ok().flatten())
            .collect();

        // Collect all entity IDs for the root + downstream
        let mut all_entity_ids = vec![entity.id.clone()];
        all_entity_ids.extend(downstream_ids);

        let affected_assertions: Vec<Assertion> = repo
            .get_assertions_for_entities(&all_entity_ids)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .collect();

        // Risk assessment: load semantic space and evaluate
        let risk_assessment = SemanticSpace::load(repo, &entity.id)
            .ok()
            .map(|semantic| semantic.assess_risk(&entity.id, &entity.qualified_name, &structure));

        // Compute per-entity assertion counts
        let downstream_assertion_counts: Vec<usize> = downstream_entities
            .iter()
            .map(|e| {
                affected_assertions
                    .iter()
                    .filter(|a| a.entity_id == e.id)
                    .count()
            })
            .collect();

        // Compute downstream coverage metrics
        let downstream_count = downstream_entities.len();
        let (downstream_coverage, blind_downstream) = if downstream_count > 0 {
            let covered_count = downstream_assertion_counts
                .iter()
                .filter(|&&c| c > 0)
                .count();
            let blind_count = downstream_count.saturating_sub(covered_count);
            let coverage = (covered_count as f64) / (downstream_count as f64);
            (Some(coverage), Some(blind_count))
        } else {
            (None, None)
        };

        Ok(ImpactCard {
            entity,
            downstream_entities,
            affected_assertions,
            downstream_assertion_counts,
            risk_assessment,
            downstream_coverage,
            blind_downstream,
        })
    }
}
