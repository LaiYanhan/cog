use std::collections::HashMap;

use crate::model::{Assertion, AssertionRelation, Entity, EntityRelation, Evidence, ModelSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDiff {
    pub entity_adds: Vec<Entity>,
    pub entity_removals: Vec<Entity>,
    pub assertion_adds: Vec<Assertion>,
    pub assertion_removals: Vec<Assertion>,
    pub assertion_changes: Vec<FieldChange<Assertion>>,
    pub evidence_adds: Vec<Evidence>,
    pub evidence_removals: Vec<Evidence>,
    pub entity_relation_adds: Vec<EntityRelation>,
    pub entity_relation_removals: Vec<EntityRelation>,
    pub assertion_relation_adds: Vec<AssertionRelation>,
    pub assertion_relation_removals: Vec<AssertionRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange<T> {
    pub before: T,
    pub after: T,
    pub changed_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffItem {
    EntityAdded(Entity),
    EntityRemoved(Entity),
    AssertionAdded(Assertion),
    AssertionRemoved(Assertion),
    AssertionChanged(FieldChange<Assertion>),
    EvidenceAdded(Evidence),
    EvidenceRemoved(Evidence),
    EntityRelationAdded(EntityRelation),
    EntityRelationRemoved(EntityRelation),
    AssertionRelationAdded(AssertionRelation),
    AssertionRelationRemoved(AssertionRelation),
}

impl ModelDiff {
    pub fn diff(base: &ModelSnapshot, branch: &ModelSnapshot) -> Self {
        let entity_adds = diff_vec_by_id(&base.entities, &branch.entities);
        let entity_removals = diff_vec_by_id(&branch.entities, &base.entities);

        let assertion_adds = diff_vec_by_id(&base.assertions, &branch.assertions);
        let assertion_removals = diff_vec_by_id(&branch.assertions, &base.assertions);

        let base_assertions: HashMap<&str, &Assertion> =
            base.assertions.iter().map(|a| (a.id.as_str(), a)).collect();
        let branch_assertions: HashMap<&str, &Assertion> = branch
            .assertions
            .iter()
            .map(|a| (a.id.as_str(), a))
            .collect();

        let mut assertion_changes = diff_modified_assertions(&base_assertions, &branch_assertions);
        assertion_changes.sort_by(|a, b| a.before.id.cmp(&b.before.id));

        let evidence_adds = diff_vec_by_id(&base.evidences, &branch.evidences);
        let evidence_removals = diff_vec_by_id(&branch.evidences, &base.evidences);

        let entity_relation_adds =
            diff_relations_by_id(&base.entity_relations, &branch.entity_relations);
        let entity_relation_removals =
            diff_relations_by_id(&branch.entity_relations, &base.entity_relations);

        let assertion_relation_adds =
            diff_relations_by_id(&base.assertion_relations, &branch.assertion_relations);
        let assertion_relation_removals =
            diff_relations_by_id(&branch.assertion_relations, &base.assertion_relations);

        Self {
            entity_adds,
            entity_removals,
            assertion_adds,
            assertion_removals,
            assertion_changes,
            evidence_adds,
            evidence_removals,
            entity_relation_adds,
            entity_relation_removals,
            assertion_relation_adds,
            assertion_relation_removals,
        }
    }

    pub fn items(&self) -> Vec<DiffItem> {
        let mut items = Vec::new();
        for e in &self.entity_adds {
            items.push(DiffItem::EntityAdded(e.clone()));
        }
        for e in &self.entity_removals {
            items.push(DiffItem::EntityRemoved(e.clone()));
        }
        for a in &self.assertion_adds {
            items.push(DiffItem::AssertionAdded(a.clone()));
        }
        for a in &self.assertion_removals {
            items.push(DiffItem::AssertionRemoved(a.clone()));
        }
        for c in &self.assertion_changes {
            items.push(DiffItem::AssertionChanged(c.clone()));
        }
        for e in &self.evidence_adds {
            items.push(DiffItem::EvidenceAdded(e.clone()));
        }
        for e in &self.evidence_removals {
            items.push(DiffItem::EvidenceRemoved(e.clone()));
        }
        for r in &self.entity_relation_adds {
            items.push(DiffItem::EntityRelationAdded(r.clone()));
        }
        for r in &self.entity_relation_removals {
            items.push(DiffItem::EntityRelationRemoved(r.clone()));
        }
        for r in &self.assertion_relation_adds {
            items.push(DiffItem::AssertionRelationAdded(r.clone()));
        }
        for r in &self.assertion_relation_removals {
            items.push(DiffItem::AssertionRelationRemoved(r.clone()));
        }
        items
    }

    pub fn is_empty(&self) -> bool {
        self.items().is_empty()
    }

    pub fn summary_counts(&self) -> DiffSummary {
        let items = self.items();
        let mut counts = DiffSummary::default();
        for item in &items {
            match item {
                DiffItem::EntityAdded(_) => counts.entities_added += 1,
                DiffItem::EntityRemoved(_) => counts.entities_removed += 1,
                DiffItem::AssertionAdded(_) => counts.assertions_added += 1,
                DiffItem::AssertionRemoved(_) => counts.assertions_removed += 1,
                DiffItem::AssertionChanged(_) => counts.assertions_changed += 1,
                DiffItem::EvidenceAdded(_) => counts.evidences_added += 1,
                DiffItem::EvidenceRemoved(_) => counts.evidences_removed += 1,
                DiffItem::EntityRelationAdded(_) => counts.entity_relations_added += 1,
                DiffItem::EntityRelationRemoved(_) => counts.entity_relations_removed += 1,
                DiffItem::AssertionRelationAdded(_) => counts.assertion_relations_added += 1,
                DiffItem::AssertionRelationRemoved(_) => counts.assertion_relations_removed += 1,
            }
        }
        counts.total = items.len();
        counts
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffSummary {
    pub total: usize,
    pub entities_added: usize,
    pub entities_removed: usize,
    pub assertions_added: usize,
    pub assertions_removed: usize,
    pub assertions_changed: usize,
    pub evidences_added: usize,
    pub evidences_removed: usize,
    pub entity_relations_added: usize,
    pub entity_relations_removed: usize,
    pub assertion_relations_added: usize,
    pub assertion_relations_removed: usize,
}

trait HasId {
    fn id(&self) -> &str;
}

impl HasId for Entity {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for Assertion {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for Evidence {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for EntityRelation {
    fn id(&self) -> &str {
        &self.id
    }
}

impl HasId for AssertionRelation {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Returns items in `newer` that are not in `older` (by id).
fn diff_vec_by_id<T: HasId + Clone>(older: &[T], newer: &[T]) -> Vec<T> {
    let older_ids: std::collections::HashSet<&str> = older.iter().map(|x| x.id()).collect();
    newer
        .iter()
        .filter(|x| !older_ids.contains(x.id()))
        .cloned()
        .collect()
}

fn diff_relations_by_id<T: HasId + Clone>(older: &[T], newer: &[T]) -> Vec<T> {
    diff_vec_by_id(older, newer)
}

fn diff_modified_assertions(
    base: &HashMap<&str, &Assertion>,
    branch: &HashMap<&str, &Assertion>,
) -> Vec<FieldChange<Assertion>> {
    let mut changes = Vec::new();
    for (id, branch_assertion) in branch {
        if let Some(base_assertion) = base.get(id) {
            let mut changed_fields = Vec::new();
            if base_assertion.kind != branch_assertion.kind {
                changed_fields.push("kind".to_string());
            }
            if base_assertion.claim != branch_assertion.claim {
                changed_fields.push("claim".to_string());
            }
            if base_assertion.status != branch_assertion.status {
                changed_fields.push("status".to_string());
            }
            if base_assertion.retraction_reason != branch_assertion.retraction_reason {
                changed_fields.push("retraction_reason".to_string());
            }
            // entity_id change = effectively a different assertion, skip
            if !changed_fields.is_empty() {
                changes.push(FieldChange {
                    before: (*base_assertion).clone(),
                    after: (*branch_assertion).clone(),
                    changed_fields,
                });
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AssertionKind, AssertionStatus, EntityKind, EntityOrigin};
    use chrono::Utc;

    fn make_entity(name: &str) -> Entity {
        Entity {
            id: uuid::Uuid::new_v4().to_string(),
            qualified_name: name.to_string(),
            kind: EntityKind::Function,
            origin: EntityOrigin::Manual,
            created_at: Utc::now(),
        }
    }

    fn make_assertion(entity_id: &str, claim: &str, status: AssertionStatus) -> Assertion {
        Assertion {
            id: uuid::Uuid::new_v4().to_string(),
            entity_id: entity_id.to_string(),
            kind: AssertionKind::Contract,
            claim: claim.to_string(),
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            retraction_reason: None,
        }
    }

    fn empty_snapshot() -> ModelSnapshot {
        ModelSnapshot {
            entities: Vec::new(),
            assertions: Vec::new(),
            evidences: Vec::new(),
            entity_relations: Vec::new(),
            assertion_relations: Vec::new(),
            changelog: Vec::new(),
        }
    }

    #[test]
    fn empty_diff_when_snapshots_identical() {
        let snap = empty_snapshot();
        let diff = ModelDiff::diff(&snap, &snap);
        assert!(diff.is_empty());
        assert_eq!(diff.items().len(), 0);
    }

    #[test]
    fn detects_entity_addition() {
        let entity = make_entity("auth::login");
        let mut branch = empty_snapshot();
        branch.entities.push(entity.clone());

        let diff = ModelDiff::diff(&empty_snapshot(), &branch);
        assert_eq!(diff.entity_adds.len(), 1);
        assert_eq!(diff.entity_adds[0].qualified_name, "auth::login");
        assert_eq!(diff.entity_removals.len(), 0);
    }

    #[test]
    fn detects_entity_removal() {
        let entity = make_entity("auth::login");
        let mut base = empty_snapshot();
        base.entities.push(entity);

        let diff = ModelDiff::diff(&base, &empty_snapshot());
        assert_eq!(diff.entity_removals.len(), 1);
        assert_eq!(diff.entity_adds.len(), 0);
    }

    #[test]
    fn detects_assertion_status_change() {
        let entity = make_entity("auth::login");
        let base_assertion = make_assertion(&entity.id, "returns token", AssertionStatus::Active);
        let mut branch_assertion = base_assertion.clone();
        branch_assertion.status = AssertionStatus::Retracted;
        branch_assertion.retraction_reason = Some("test".to_string());

        let base = ModelSnapshot {
            assertions: vec![base_assertion],
            ..empty_snapshot()
        };
        let branch = ModelSnapshot {
            assertions: vec![branch_assertion],
            ..empty_snapshot()
        };

        let diff = ModelDiff::diff(&base, &branch);
        assert_eq!(diff.assertion_changes.len(), 1);
        assert!(
            diff.assertion_changes[0]
                .changed_fields
                .contains(&"status".to_string())
        );
        assert!(
            diff.assertion_changes[0]
                .changed_fields
                .contains(&"retraction_reason".to_string())
        );
    }

    #[test]
    fn detects_assertion_addition_and_removal() {
        let entity = make_entity("auth::login");
        let a1 = make_assertion(&entity.id, "claim1", AssertionStatus::Active);
        let a2 = make_assertion(&entity.id, "claim2", AssertionStatus::Active);

        let base = ModelSnapshot {
            assertions: vec![a1],
            ..empty_snapshot()
        };
        let branch = ModelSnapshot {
            assertions: vec![a2],
            ..empty_snapshot()
        };

        let diff = ModelDiff::diff(&base, &branch);
        assert_eq!(diff.assertion_adds.len(), 1);
        assert_eq!(diff.assertion_removals.len(), 1);
        assert_eq!(diff.assertion_changes.len(), 0);
    }

    #[test]
    fn items_are_indexed_in_consistent_order() {
        let e1 = make_entity("a");
        let e2 = make_entity("b");
        let mut base = empty_snapshot();
        base.entities.push(e1);
        let mut branch = empty_snapshot();
        branch.entities.push(e2);

        let diff = ModelDiff::diff(&base, &branch);
        let items = diff.items();
        // items() lists adds before removals
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], DiffItem::EntityAdded(_)));
        assert!(matches!(items[1], DiffItem::EntityRemoved(_)));
    }

    #[test]
    fn summary_counts_are_accurate() {
        let e1 = make_entity("a");
        let e2 = make_entity("b");
        let a1 = make_assertion(&e1.id, "c1", AssertionStatus::Active);
        let mut a2 = make_assertion(&e1.id, "c2", AssertionStatus::Active);
        a2.id = a1.id.clone(); // Same ID = modification
        a2.status = AssertionStatus::Retracted;

        let base = ModelSnapshot {
            entities: vec![e1],
            assertions: vec![a1],
            ..empty_snapshot()
        };
        let branch = ModelSnapshot {
            entities: vec![e2],
            assertions: vec![a2],
            ..empty_snapshot()
        };

        let diff = ModelDiff::diff(&base, &branch);
        let summary = diff.summary_counts();
        assert_eq!(summary.entities_added, 1);
        assert_eq!(summary.entities_removed, 1);
        assert_eq!(summary.assertions_changed, 1);
        assert_eq!(summary.total, 3);
    }
}
