use std::path::Path;

use anyhow::Result;

use crate::domain::*;

/// Persistence contract for the cognitive model.
///
/// Single production implementation: [`SqliteRepository`](super::SqliteRepository).
pub trait Repository {
    // ── Entity ──────────────────────────────────────────────────────────────

    fn upsert_entity(&self, name: &str, kind: EntityKind, origin: EntityOrigin) -> Result<Entity>;

    /// Ensure a manually-created entity exists, inferring its [`EntityKind`].
    fn ensure_manual_entity(&self, name: &str) -> Result<Entity> {
        let kind = EntityKind::infer(name);
        self.upsert_entity(name, kind, EntityOrigin::Manual)
    }
    fn get_entity(&self, id: &str) -> Result<Option<Entity>>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>>;
    /// Resolve an entity by exact qualified name, falling back to suffix matching.
    /// Returns the entity if exactly one match is found, or an error with suggestions.
    fn resolve_entity(&self, name: &str) -> Result<Entity>;
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
    fn get_assertion(&self, id: &str) -> Result<Option<Assertion>>;
    fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>>;
    fn get_assertions_for_entities(&self, entity_ids: &[String]) -> Result<Vec<Assertion>>;
    fn list_assertions(&self) -> Result<Vec<Assertion>>;
    fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()>;
    fn retract_assertion(&self, id: &str, reason: &str) -> Result<()>;
    fn resolve_assertion_id(&self, id: &str) -> Result<String>;

    // ── Evidence ────────────────────────────────────────────────────────────

    fn get_evidence_for_assertion(&self, assertion_id: &str) -> Result<Vec<Evidence>>;
    fn get_evidences_for_assertions(&self, assertion_ids: &[String]) -> Result<Vec<Evidence>>;
    fn list_evidences(&self) -> Result<Vec<Evidence>>;

    // ── Relations ───────────────────────────────────────────────────────────

    fn add_entity_relation(&self, from: &str, to: &str, kind: EntityRelationKind) -> Result<()>;
    fn list_entity_relations(&self) -> Result<Vec<EntityRelation>>;
    fn list_assertion_relations(&self) -> Result<Vec<AssertionRelation>>;
    /// Load assertion relations where both endpoints are in the given set.
    /// More efficient than list_assertion_relations + filter for large models.
    fn get_assertion_relations_for(
        &self,
        assertion_ids: &[String],
    ) -> Result<Vec<AssertionRelation>>;
    fn get_dependents(&self, assertion_id: &str) -> Result<Vec<Assertion>>;
    fn get_dependencies(&self, assertion_id: &str) -> Result<Vec<Assertion>>;
    fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>>;

    // ── Scanning ────────────────────────────────────────────────────────────

    fn get_scanned_entity_names(&self) -> Result<Vec<String>>;

    /// Get names of all Experiment-origin entities (provisional entities created
    /// by experiment commit that haven't been promoted to Scan by sync yet).
    fn get_experiment_entity_names(&self) -> Result<Vec<String>>;

    // ── Changelog ───────────────────────────────────────────────────────────

    fn append_changelog(
        &self,
        action: ChangelogAction,
        target_id: &str,
        detail: &str,
    ) -> Result<()>;
    fn list_changelog_entries(&self) -> Result<Vec<ChangelogEntry>>;

    // ── Metrics ────────────────────────────────────────────────────────────

    fn update_entity_metrics(
        &self,
        id: &str,
        metrics: &crate::domain::metrics::EntityMetrics,
    ) -> Result<()>;

    // ── Utility ─────────────────────────────────────────────────────────────

    fn count_relations_for_entity(&self, entity_id: &str) -> Result<u64>;
    fn stats(&self) -> Result<ModelStats>;
    fn vacuum_into(&self, target_path: &Path) -> Result<()>;
}
