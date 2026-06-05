use std::path::Path;

use anyhow::Result;

use crate::domain::*;

/// Persistence contract for the cognitive model.
///
/// Single production implementation: [`SqliteRepository`](super::SqliteRepository).
/// Tests use `SqliteRepository::open_in_memory()`.
#[allow(dead_code)]
pub trait Repository {
    // ── Entity ──────────────────────────────────────────────────────────────

    fn upsert_entity(&self, name: &str, kind: EntityKind, origin: EntityOrigin) -> Result<Entity>;
    fn insert_entity(&self, entity: &Entity) -> Result<bool>;
    fn get_entity(&self, id: &str) -> Result<Option<Entity>>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>>;
    fn list_entities(&self) -> Result<Vec<Entity>>;
    fn list_entities_filtered(
        &self,
        kind: Option<EntityKind>,
        origin: Option<EntityOrigin>,
        prefix: Option<&str>,
    ) -> Result<Vec<(Entity, usize)>>;
    fn delete_entity(&self, qualified_name: &str) -> Result<bool>;

    // ── Assertion ───────────────────────────────────────────────────────────

    fn create_assertion(
        &self,
        entity_id: &str,
        kind: AssertionKind,
        claim: &str,
        grounds: &str,
        depends_on: Option<&str>,
    ) -> Result<Assertion>;
    fn insert_assertion(&self, assertion: &Assertion) -> Result<bool>;
    fn get_assertion(&self, id: &str) -> Result<Option<Assertion>>;
    fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>>;
    fn get_assertions_for_entities(&self, entity_ids: &[String]) -> Result<Vec<Assertion>>;
    fn list_assertions(&self) -> Result<Vec<Assertion>>;
    fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()>;
    fn retract_assertion(&self, id: &str, reason: &str) -> Result<()>;
    fn resolve_assertion_id(&self, id: &str) -> Result<String>;

    // ── Evidence ────────────────────────────────────────────────────────────

    fn create_evidence(&self, assertion_id: &str, source: &str, detail: &str) -> Result<Evidence>;
    fn insert_evidence(&self, evidence: &Evidence) -> Result<bool>;
    fn get_evidence(&self, id: &str) -> Result<Option<Evidence>>;
    fn get_evidence_for_assertion(&self, assertion_id: &str) -> Result<Vec<Evidence>>;
    fn list_evidences(&self) -> Result<Vec<Evidence>>;

    // ── Relations ───────────────────────────────────────────────────────────

    fn add_entity_relation(&self, from: &str, to: &str, kind: EntityRelationKind) -> Result<()>;
    fn list_entity_relations(&self) -> Result<Vec<EntityRelation>>;
    fn add_assertion_dependency(&self, from_assertion: &str, to_assertion: &str) -> Result<()>;
    fn list_assertion_relations(&self) -> Result<Vec<AssertionRelation>>;
    fn get_dependents(&self, assertion_id: &str) -> Result<Vec<Assertion>>;
    fn get_dependencies(&self, assertion_id: &str) -> Result<Vec<Assertion>>;
    fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>>;
    fn get_impact_neighbors(&self, entity_id: &str) -> Result<Vec<Entity>>;

    // ── Scanning ────────────────────────────────────────────────────────────

    fn get_scanned_entity_names(&self) -> Result<Vec<String>>;

    // ── Changelog ───────────────────────────────────────────────────────────

    fn append_changelog(
        &self,
        action: ChangelogAction,
        target_id: &str,
        detail: &str,
    ) -> Result<()>;
    fn list_changelog_entries(&self) -> Result<Vec<ChangelogEntry>>;

    // ── Utility ─────────────────────────────────────────────────────────────

    fn count_relations_for_entity(&self, entity_id: &str) -> Result<u64>;
    fn stats(&self) -> Result<ModelStats>;
    fn count_unasserted_entities(&self) -> Result<u64>;
    fn vacuum_into(&self, target_path: &Path) -> Result<()>;
}
