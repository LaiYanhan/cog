use std::path::Path;

use anyhow::Result;

use crate::domain::*;
use crate::repo::Repository;

use super::SqliteRepository;

impl Repository for SqliteRepository {
    fn upsert_entity(&self, name: &str, kind: EntityKind, origin: EntityOrigin) -> Result<Entity> {
        self.upsert_entity(name, kind, origin)
    }

    fn get_entity(&self, id: &str) -> Result<Option<Entity>> {
        self.get_entity(id)
    }

    fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>> {
        self.get_entity_by_name(name)
    }

    fn list_entities(&self) -> Result<Vec<Entity>> {
        self.list_entities()
    }

    fn list_entities_filtered(
        &self,
        kind: Option<EntityKind>,
        origin: Option<EntityOrigin>,
        prefix: Option<&str>,
    ) -> Result<Vec<(Entity, usize)>> {
        self.list_entities_filtered(kind, origin, prefix)
    }

    fn delete_entity(&self, qualified_name: &str) -> Result<bool> {
        self.delete_entity(qualified_name)
    }

    fn create_assertion(
        &self,
        entity_id: &str,
        kind: AssertionKind,
        claim: &str,
        grounds: &str,
        depends_on: Option<&str>,
    ) -> Result<Assertion> {
        self.create_assertion(entity_id, kind, claim, grounds, depends_on)
    }

    fn get_assertion(&self, id: &str) -> Result<Option<Assertion>> {
        self.get_assertion(id)
    }

    fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>> {
        self.get_assertions_for_entity(entity_id)
    }

    fn get_assertions_for_entities(&self, entity_ids: &[String]) -> Result<Vec<Assertion>> {
        self.get_assertions_for_entities(entity_ids)
    }

    fn list_assertions(&self) -> Result<Vec<Assertion>> {
        self.list_assertions()
    }

    fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()> {
        self.update_assertion_status(id, status)
    }

    fn retract_assertion(&self, id: &str, reason: &str) -> Result<()> {
        self.retract_assertion(id, reason)
    }

    fn resolve_assertion_id(&self, id: &str) -> Result<String> {
        self.resolve_assertion_id(id)
    }

    fn get_evidence_for_assertion(&self, assertion_id: &str) -> Result<Vec<Evidence>> {
        self.get_evidence_for_assertion(assertion_id)
    }

    fn list_evidences(&self) -> Result<Vec<Evidence>> {
        self.list_evidences()
    }

    fn add_entity_relation(&self, from: &str, to: &str, kind: EntityRelationKind) -> Result<()> {
        self.add_entity_relation(from, to, kind)
    }

    fn list_entity_relations(&self) -> Result<Vec<EntityRelation>> {
        self.list_entity_relations()
    }

    fn list_assertion_relations(&self) -> Result<Vec<AssertionRelation>> {
        self.list_assertion_relations()
    }

    fn get_dependents(&self, assertion_id: &str) -> Result<Vec<Assertion>> {
        self.get_dependents(assertion_id)
    }

    fn get_dependencies(&self, assertion_id: &str) -> Result<Vec<Assertion>> {
        self.get_dependencies(assertion_id)
    }

    fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>> {
        self.get_related_entities(entity_id)
    }

    fn get_scanned_entity_names(&self) -> Result<Vec<String>> {
        self.get_scanned_entity_names()
    }

    fn append_changelog(
        &self,
        action: ChangelogAction,
        target_id: &str,
        detail: &str,
    ) -> Result<()> {
        self.append_changelog(action, target_id, detail)
    }

    fn list_changelog_entries(&self) -> Result<Vec<ChangelogEntry>> {
        self.list_changelog_entries()
    }

    fn count_relations_for_entity(&self, entity_id: &str) -> Result<u64> {
        self.count_relations_for_entity(entity_id)
    }

    fn stats(&self) -> Result<ModelStats> {
        self.stats()
    }
    fn count_unasserted_entities(&self) -> Result<u64> {
        self.count_unasserted_entities()
    }

    fn vacuum_into(&self, target_path: &Path) -> Result<()> {
        self.vacuum_into(target_path)
    }

    fn update_entity_metrics(
        &self,
        id: &str,
        metrics: &crate::domain::metrics::EntityMetrics,
    ) -> Result<()> {
        self.update_entity_metrics(id, metrics)
    }
}
