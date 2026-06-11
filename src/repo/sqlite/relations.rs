use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use rusqlite::params;
use uuid::Uuid;

use crate::domain::{
    AssertionRelation, AssertionRelationKind, EntityRelation, EntityRelationKind, RelatedEntity,
    RelationDirection,
};

use super::SqliteRepository;
use super::helpers::*;

impl SqliteRepository {
    pub(super) fn add_entity_relation(
        &self,
        from: &str,
        to: &str,
        kind: EntityRelationKind,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO entity_relations (id, from_entity, to_entity, kind) VALUES (?1, ?2, ?3, ?4)",
                params![Uuid::new_v4().to_string(), from, to, kind.to_string()],
            )
            .with_context(|| format!("failed to add entity relation: {from} -> {to}"))?;

        Ok(())
    }

    pub(super) fn list_entity_relations(&self) -> Result<Vec<EntityRelation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, from_entity, to_entity, kind FROM entity_relations ORDER BY from_entity, to_entity")
            .context("failed to prepare list_entity_relations statement")?;
        let mut rows = stmt.query([]).context("failed to query entity_relations")?;

        let mut relations = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate entity_relations")? {
            let kind: String = row.get(3)?;
            relations.push(EntityRelation {
                id: row.get(0)?,
                from_entity: row.get(1)?,
                to_entity: row.get(2)?,
                kind: EntityRelationKind::from_str(&kind)
                    .map_err(|_| anyhow!("invalid entity relation kind in db: {kind}"))?,
            });
        }

        Ok(relations)
    }

    pub(super) fn add_assertion_dependency(
        &self,
        from_assertion: &str,
        to_assertion: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO assertion_relations (id, from_assertion, to_assertion, kind) VALUES (?1, ?2, ?3, ?4)",
                params![
                    Uuid::new_v4().to_string(),
                    from_assertion,
                    to_assertion,
                    AssertionRelationKind::DependsOn.to_string()
                ],
            )
            .with_context(|| {
                format!("failed to add assertion dependency: {from_assertion} -> {to_assertion}")
            })?;

        Ok(())
    }

    pub(super) fn list_assertion_relations(&self) -> Result<Vec<AssertionRelation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, from_assertion, to_assertion, kind FROM assertion_relations ORDER BY from_assertion, to_assertion",
            )
            .context("failed to prepare list_assertion_relations statement")?;
        let mut rows = stmt
            .query([])
            .context("failed to query assertion_relations")?;

        let mut relations = Vec::new();
        while let Some(row) = rows
            .next()
            .context("failed to iterate assertion_relations")?
        {
            let kind: String = row.get(3)?;
            relations.push(AssertionRelation {
                id: row.get(0)?,
                from_assertion: row.get(1)?,
                to_assertion: row.get(2)?,
                kind: AssertionRelationKind::from_str(&kind)
                    .map_err(|_| anyhow!("invalid assertion relation kind in db: {kind}"))?,
            });
        }

        Ok(relations)
    }

    pub(super) fn get_assertion_relations_for(
        &self,
        assertion_ids: &[String],
    ) -> Result<Vec<AssertionRelation>> {
        if assertion_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Build a temporary table of target IDs and join — avoids
        // SQLite's 999-parameter limit for very large models.
        self.conn
            .execute_batch("CREATE TEMP TABLE IF NOT EXISTS _target_ids(id TEXT PRIMARY KEY); DELETE FROM _target_ids;")
            .context("failed to prepare temp table")?;
        {
            let mut ins = self
                .conn
                .prepare("INSERT OR IGNORE INTO _target_ids VALUES (?1)")
                .context("failed to prepare temp insert")?;
            for id in assertion_ids {
                ins.execute(params![id])?;
            }
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT r.id, r.from_assertion, r.to_assertion, r.kind
                 FROM assertion_relations r
                 WHERE EXISTS (SELECT 1 FROM _target_ids WHERE id = r.from_assertion)
                   AND EXISTS (SELECT 1 FROM _target_ids WHERE id = r.to_assertion)",
            )
            .context("failed to prepare get_assertion_relations_for statement")?;
        let mut rows = stmt.query([])?;
        let mut relations = Vec::new();
        while let Some(row) = rows
            .next()
            .context("failed to iterate assertion relations")?
        {
            let kind: String = row.get(3)?;
            relations.push(AssertionRelation {
                id: row.get(0)?,
                from_assertion: row.get(1)?,
                to_assertion: row.get(2)?,
                kind: AssertionRelationKind::from_str(&kind)
                    .map_err(|_| anyhow!("invalid assertion relation kind in db: {kind}"))?,
            });
        }
        Ok(relations)
    }

    pub(super) fn get_dependents(
        &self,
        assertion_id: &str,
    ) -> Result<Vec<crate::domain::Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT a.id, a.entity_id, a.kind, a.claim, a.status, a.created_at, a.updated_at, a.retraction_reason
                 FROM assertion_relations r
                 JOIN assertions a ON a.id = r.from_assertion
                 WHERE r.to_assertion = ?1 AND r.kind = 'depends_on'
                 ORDER BY a.created_at",
            )
            .context("failed to prepare get_dependents statement")?;

        let mut rows = stmt
            .query(params![assertion_id])
            .context("failed to query dependents")?;

        let mut assertions = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate dependents")? {
            assertions.push(map_assertion_row(row)?);
        }

        Ok(assertions)
    }

    pub(super) fn get_dependencies(
        &self,
        assertion_id: &str,
    ) -> Result<Vec<crate::domain::Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT a.id, a.entity_id, a.kind, a.claim, a.status, a.created_at, a.updated_at, a.retraction_reason
                 FROM assertion_relations r
                 JOIN assertions a ON a.id = r.to_assertion
                 WHERE r.from_assertion = ?1 AND r.kind = 'depends_on'
                 ORDER BY a.created_at",
            )
            .context("failed to prepare get_dependencies statement")?;

        let mut rows = stmt
            .query(params![assertion_id])
            .context("failed to query dependencies")?;

        let mut assertions = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate dependencies")? {
            assertions.push(map_assertion_row(row)?);
        }

        Ok(assertions)
    }

    pub(super) fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>> {
        let mut related = Vec::new();

        let mut out_stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.metrics_json, e.created_at, r.kind
                 FROM entity_relations r
                 JOIN entities e ON e.id = r.to_entity
                 WHERE r.from_entity = ?1",
            )
            .context("failed to prepare outgoing relations query")?;
        let mut out_rows = out_stmt
            .query(params![entity_id])
            .context("failed to query outgoing related entities")?;
        while let Some(row) = out_rows
            .next()
            .context("failed to iterate outgoing entities")?
        {
            let relation_kind: String = row.get(6)?;
            related.push(RelatedEntity {
                entity: map_entity_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                )?,
                kind: EntityRelationKind::from_str(&relation_kind)
                    .map_err(|_| anyhow!("invalid entity relation kind in db: {relation_kind}"))?,
                direction: RelationDirection::Outgoing,
            });
        }

        let mut in_stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.metrics_json, e.created_at, r.kind
                 FROM entity_relations r
                 JOIN entities e ON e.id = r.from_entity
                 WHERE r.to_entity = ?1",
            )
            .context("failed to prepare incoming relations query")?;
        let mut in_rows = in_stmt
            .query(params![entity_id])
            .context("failed to query incoming related entities")?;
        while let Some(row) = in_rows
            .next()
            .context("failed to iterate incoming entities")?
        {
            let relation_kind: String = row.get(6)?;
            related.push(RelatedEntity {
                entity: map_entity_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                )?,
                kind: EntityRelationKind::from_str(&relation_kind)
                    .map_err(|_| anyhow!("invalid entity relation kind in db: {relation_kind}"))?,
                direction: RelationDirection::Incoming,
            });
        }

        Ok(related)
    }

    pub(super) fn count_relations_for_entity(&self, entity_id: &str) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entity_relations WHERE from_entity = ?1 OR to_entity = ?1",
                params![entity_id],
                |row| row.get(0),
            )
            .context("failed to count entity relations")?;
        Ok(count as u64)
    }
}
