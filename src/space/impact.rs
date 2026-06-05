use std::collections::{HashSet, VecDeque};

use anyhow::{Result, anyhow};

use crate::domain::*;
use crate::repo::Repository;

pub struct ImpactEngine;

impl ImpactEngine {
    pub fn analyze(repo: &dyn Repository, entity_name: &str) -> Result<ImpactCard> {
        let entity = repo
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
            for next in repo.get_impact_neighbors(&current)? {
                queue.push_back(next.id);
            }
        }

        let downstream_entities = entity_ids
            .iter()
            .filter(|id| *id != &entity.id)
            .map(|id| repo.get_entity(id))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let affected_assertions = repo
            .get_assertions_for_entities(&entity_ids)?
            .into_iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .collect();

        Ok(ImpactCard {
            entity,
            downstream_entities,
            affected_assertions,
        })
    }
}
