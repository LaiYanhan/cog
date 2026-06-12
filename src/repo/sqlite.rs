mod assertions;
mod changelog;
mod entities;
mod evidence;
mod helpers;
mod relations;
mod stats;

#[cfg(test)]
mod tests;

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

use helpers::SCHEMA;

#[derive(Debug)]
pub struct SqliteRepository {
    pub(crate) conn: Connection,
}

impl SqliteRepository {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db: {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .context("failed to configure sqlite pragmas")?;
        conn.execute_batch(SCHEMA)
            .context("failed to initialize sqlite schema")?;

        // Migration: add origin column if upgrading from a pre-origin schema
        let has_origin: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('entities') WHERE name = 'origin'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_origin {
            conn.execute_batch(
                "ALTER TABLE entities ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'",
            )
            .context("failed to migrate entities table: add origin column")?;
        }

        // Migration: add metrics_json column for EntityMetrics
        let has_metrics: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM pragma_table_info('entities') WHERE name = 'metrics_json'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if !has_metrics {
            conn.execute_batch("ALTER TABLE entities ADD COLUMN metrics_json TEXT")
                .context("failed to migrate entities table: add metrics_json column")?;
        }

        Ok(Self { conn })
    }

    /// Open an in-memory SQLite database for testing.
    /// Zero disk I/O, full SQL semantics (FK constraints, transactions, etc).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().context("failed to open in-memory sqlite db")?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .context("failed to configure sqlite pragmas")?;
        conn.execute_batch(SCHEMA)
            .context("failed to initialize sqlite schema")?;
        Ok(Self { conn })
    }

    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .context("failed to begin transaction")?;

        match f() {
            Ok(value) => {
                self.conn
                    .execute_batch("COMMIT")
                    .context("failed to commit transaction")?;
                Ok(value)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    /// Checkpoint the WAL so all pending writes are flushed to the main DB file.
    /// Must be called before any external file-level operations on the DB.
    pub fn checkpoint_wal(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .context("failed to checkpoint WAL")
    }
}

use crate::domain::*;
use crate::repo::Repository;

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
    fn resolve_entity(&self, name: &str) -> Result<Entity> {
        self.resolve_entity(name)
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
    fn get_evidences_for_assertions(&self, assertion_ids: &[String]) -> Result<Vec<Evidence>> {
        self.get_evidences_for_assertions(assertion_ids)
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
    fn get_assertion_relations_for(
        &self,
        assertion_ids: &[String],
    ) -> Result<Vec<AssertionRelation>> {
        self.get_assertion_relations_for(assertion_ids)
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
    fn get_experiment_entity_names(&self) -> Result<Vec<String>> {
        self.get_experiment_entity_names()
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
