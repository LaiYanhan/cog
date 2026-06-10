use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::Result;

use crate::domain::{Entity, EntityRelation, EntityRelationKind};
use crate::repo::Repository;

// ---------------------------------------------------------------------------
// Node & Edge types
// ---------------------------------------------------------------------------

/// A node in the structure sub-space: an entity plus its adjacency metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNode {
    pub entity: Entity,
    /// IDs of entities that depend on this one (incoming edges).
    pub dependents: Vec<String>,
    /// IDs of entities this one depends on (outgoing edges).
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructureEdge {
    pub from: String,

    pub to: String,
    pub kind: EntityRelationKind,
}

// ---------------------------------------------------------------------------
// StructureSpace
// ---------------------------------------------------------------------------

/// Read-only view of the structural sub-space (§2.5.1).
///
/// Loaded from a Repository into memory for offline analysis and simulation.
/// Does not hold a database connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructureSpace {
    pub entities: HashMap<String, EntityNode>,
    pub edges: Vec<StructureEdge>,
    /// Entities on the boundary of the loaded subgraph (partial data).
    pub boundary: Vec<String>,
}

impl StructureSpace {
    // ── Loading ──────────────────────────────────────────────────────────

    /// Load a subgraph centred on `focus`, expanding via entity relations using
    /// BFS up to `max_depth` hops or `max_nodes` entities (whichever is hit
    /// first).  Defaults to `max_depth = 3`, `max_nodes = 500` when set to 0.
    ///
    /// The caller is responsible for resolving the focus entity (e.g. via
    /// `repo.resolve_entity()`).
    pub fn load(
        repo: &dyn Repository,
        focus: &Entity,
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Self> {
        // When max_depth=0, no depth limit — expand until max_nodes or natural boundary.
        let depth_limit = if max_depth == 0 {
            usize::MAX
        } else {
            max_depth
        };
        let cap = if max_nodes == 0 { 500 } else { max_nodes };

        // BFS: (entity_id, hop_distance)
        let mut visited: HashSet<String> = HashSet::new();
        let mut frontier: VecDeque<(String, usize)> = VecDeque::new();
        frontier.push_back((focus.id.clone(), 0));
        visited.insert(focus.id.clone());

        let mut entities: HashMap<String, Entity> = HashMap::new();
        let mut edges: Vec<StructureEdge> = Vec::new();
        let mut boundary = Vec::new();

        // Collect all entity relations once to avoid repeated queries
        let all_relations: Vec<EntityRelation> = repo.list_entity_relations()?;
        // Build adjacency index: entity_id → [(neighbour_id, kind, direction)]
        // direction = outgoing (entity → neighbour) or incoming (neighbour → entity)
        let mut outgoing: HashMap<String, Vec<(String, EntityRelationKind)>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<(String, EntityRelationKind)>> = HashMap::new();
        for rel in &all_relations {
            outgoing
                .entry(rel.from_entity.clone())
                .or_default()
                .push((rel.to_entity.clone(), rel.kind));
            incoming
                .entry(rel.to_entity.clone())
                .or_default()
                .push((rel.from_entity.clone(), rel.kind));
        }

        while let Some((current_id, hop)) = frontier.pop_front() {
            // Load entity if not already cached
            if !entities.contains_key(&current_id)
                && let Some(entity) = repo.get_entity(&current_id)?
            {
                entities.insert(current_id.clone(), entity);
            }

            if hop >= depth_limit {
                continue;
            }

            // Expand outgoing edges
            if let Some(neighbors) = outgoing.get(&current_id) {
                for (neighbor_id, kind) in neighbors {
                    edges.push(StructureEdge {
                        from: current_id.clone(),
                        to: neighbor_id.clone(),
                        kind: *kind,
                    });

                    if visited.len() >= cap {
                        if !visited.contains(neighbor_id) {
                            boundary.push(neighbor_id.clone());
                        }
                        continue;
                    }

                    if visited.insert(neighbor_id.clone()) {
                        frontier.push_back((neighbor_id.clone(), hop + 1));
                    }
                }
            }

            // Expand incoming edges (reverse direction for full neighbourhood)
            if let Some(neighbors) = incoming.get(&current_id) {
                for (neighbor_id, kind) in neighbors {
                    edges.push(StructureEdge {
                        from: neighbor_id.clone(),
                        to: current_id.clone(),
                        kind: *kind,
                    });

                    if visited.len() >= cap {
                        if !visited.contains(neighbor_id) {
                            boundary.push(neighbor_id.clone());
                        }
                        continue;
                    }

                    if visited.insert(neighbor_id.clone()) {
                        frontier.push_back((neighbor_id.clone(), hop + 1));
                    }
                }
            }
        }

        // Build EntityNode map with adjacency info
        let mut node_map: HashMap<String, EntityNode> = HashMap::new();
        for (id, entity) in entities {
            let deps: Vec<String> = outgoing
                .get(&id)
                .map(|v| v.iter().map(|(n, _)| n.clone()).collect())
                .unwrap_or_default();
            let dependents: Vec<String> = incoming
                .get(&id)
                .map(|v| v.iter().map(|(n, _)| n.clone()).collect())
                .unwrap_or_default();
            node_map.insert(
                id,
                EntityNode {
                    entity,
                    dependents,
                    dependencies: deps,
                },
            );
        }

        Ok(Self {
            entities: node_map,
            edges,
            boundary,
        })
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Entities that directly depend on `entity` via edges of a specific kind
    /// (incoming edges where `edge.to == entity_id`).
    pub fn dependents_of_kind(
        &self,
        entity_id: &str,
        kind: EntityRelationKind,
    ) -> Vec<&EntityNode> {
        self.edges
            .iter()
            .filter(|e| e.to == entity_id && e.kind == kind)
            .filter_map(|e| self.entities.get(&e.from))
            .collect()
    }
}
