use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::domain::metrics::EntityMetrics;
use crate::domain::{Entity, EntityKind, EntityOrigin};

use super::SqliteRepository;
use super::helpers::*;

impl SqliteRepository {
    pub(super) fn upsert_entity(
        &self,
        qualified_name: &str,
        kind: EntityKind,
        origin: EntityOrigin,
    ) -> Result<Entity> {
        if let Some(entity) = self.get_entity_by_name(qualified_name)? {
            return Ok(entity);
        }

        let entity = Entity {
            id: Uuid::new_v4().to_string(),
            qualified_name: qualified_name.to_string(),
            kind,
            origin,
            metrics: EntityMetrics::empty(),
            created_at: Utc::now(),
        };

        self.conn
            .execute(
                "INSERT INTO entities (id, qualified_name, kind, origin, metrics_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entity.id,
                    entity.qualified_name,
                    entity.kind.to_string(),
                    entity.origin.to_string(),
                    serde_json::to_string(&entity.metrics).unwrap_or_default(),
                    to_ts(entity.created_at)
                ],
            )
            .with_context(|| format!("failed to insert entity: {qualified_name}"))?;

        Ok(entity)
    }

    pub(super) fn get_entity(&self, id: &str) -> Result<Option<Entity>> {
        self.conn
            .query_row(
                "SELECT id, qualified_name, kind, origin, metrics_json, created_at FROM entities WHERE id = ?1",
                params![id],
                entity_from_query_row,
            )
            .optional()
            .context("failed to fetch entity by id")?
            .map(|(id, name, kind, origin, metrics_json, ts)| map_entity_row(id, name, kind, origin, metrics_json, ts))
            .transpose()
    }

    pub(super) fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>> {
        self.conn
            .query_row(
                "SELECT id, qualified_name, kind, origin, metrics_json, created_at FROM entities WHERE qualified_name = ?1",
                params![name],
                entity_from_query_row,
            )
            .optional()
            .context("failed to fetch entity by name")?
            .map(|(id, name, kind, origin, metrics_json, ts)| map_entity_row(id, name, kind, origin, metrics_json, ts))
            .transpose()
    }

    pub(super) fn list_entities(&self) -> Result<Vec<Entity>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, qualified_name, kind, origin, metrics_json, created_at FROM entities ORDER BY qualified_name",
            )
            .context("failed to prepare list_entities statement")?;

        let mut rows = stmt.query([]).context("failed to query entities")?;
        let mut entities = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate entities")? {
            entities.push(map_entity_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            )?);
        }

        Ok(entities)
    }

    /// List entities with assertion counts, optionally filtered by kind, origin, and/or name prefix.
    pub(super) fn list_entities_filtered(
        &self,
        kind: Option<EntityKind>,
        origin: Option<EntityOrigin>,
        prefix: Option<&str>,
    ) -> Result<Vec<(Entity, usize)>> {
        let mut sql = String::from(
            "SELECT e.id, e.qualified_name, e.kind, e.origin, e.metrics_json, e.created_at, \
             COUNT(a.id) AS assertion_count \
             FROM entities e \
             LEFT JOIN assertions a ON a.entity_id = e.id AND a.status = 'active'",
        );
        let mut clauses: Vec<String> = Vec::new();
        let mut p: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(k) = kind {
            clauses.push("e.kind = ?".into());
            p.push(Box::new(k.to_string()));
        }
        if let Some(o) = origin {
            clauses.push("e.origin = ?".into());
            p.push(Box::new(o.to_string()));
        }
        if let Some(px) = prefix {
            clauses.push("e.qualified_name LIKE ?".into());
            p.push(Box::new(format!("{}%", px)));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" GROUP BY e.id ORDER BY assertion_count DESC, e.qualified_name");

        let refs: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|x| x.as_ref()).collect();
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare filtered entity list")?;
        let mut rows = stmt
            .query(refs.as_slice())
            .context("failed to query filtered entities")?;

        let mut result = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate filtered entities")? {
            let entity = map_entity_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            )?;
            let count: usize = row.get::<_, i64>(6)? as usize;
            result.push((entity, count));
        }
        Ok(result)
    }

    pub(super) fn update_entity_metrics(&self, id: &str, metrics: &EntityMetrics) -> Result<()> {
        let json = serde_json::to_string(metrics)?;
        self.conn
            .execute(
                "UPDATE entities SET metrics_json = ?1 WHERE id = ?2",
                params![json, id],
            )
            .context("failed to update entity metrics")?;
        Ok(())
    }

    /// Returns the qualified names of all entities that were created by automated scanning
    /// (origin = "scan"). Used by verify --scan to detect stale entities.
    pub(super) fn get_scanned_entity_names(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT qualified_name FROM entities WHERE origin = 'scan' ORDER BY qualified_name",
            )
            .context("failed to prepare get_scanned_entity_names statement")?;

        let mut rows = stmt
            .query([])
            .context("failed to query scanned entity names")?;

        let mut names = Vec::new();
        while let Some(row) = rows
            .next()
            .context("failed to iterate scanned entity names")?
        {
            names.push(row.get(0)?);
        }

        Ok(names)
    }

    /// Delete an entity and all associated data (assertions, evidence, relations, changelog).
    /// This is a destructive operation — all cross-references are removed.
    /// Returns Ok(false) if the entity does not exist.
    pub(super) fn delete_entity(&self, qualified_name: &str) -> Result<bool> {
        let entity = match self.get_entity_by_name(qualified_name)? {
            Some(e) => e,
            None => return Ok(false),
        };
        let entity_id = &entity.id;

        // Get all assertion IDs for the entity
        let assertions = self.get_assertions_for_entity(entity_id)?;
        let assertion_ids: Vec<String> = assertions.iter().map(|a| a.id.clone()).collect();

        fn in_clause(ids: &[String]) -> String {
            (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(", ")
        }

        self.transaction(|| {
            if !assertion_ids.is_empty() {
                let placeholders = in_clause(&assertion_ids);

                // Delete evidence for all assertions
                let sql = format!("DELETE FROM evidences WHERE assertion_id IN ({placeholders})");
                self.conn.execute(&sql, rusqlite::params_from_iter(assertion_ids.iter()))?;

                // Delete assertion relations involving these assertions
                let double = format!(
                    "DELETE FROM assertion_relations WHERE from_assertion IN ({p}) OR to_assertion IN ({p})",
                    p = placeholders
                );
                self.conn.execute(
                    &double,
                    rusqlite::params_from_iter(
                        assertion_ids.iter().chain(assertion_ids.iter()),
                    ),
                )?;

                // Delete assertions
                let sql = format!("DELETE FROM assertions WHERE id IN ({placeholders})");
                self.conn.execute(&sql, rusqlite::params_from_iter(assertion_ids.iter()))?;

                // Delete changelog entries for assertions
                let sql = format!("DELETE FROM changelog WHERE target_id IN ({placeholders})");
                self.conn.execute(&sql, rusqlite::params_from_iter(assertion_ids.iter()))?;
            }

            // Delete entity relations
            self.conn
                .execute(
                    "DELETE FROM entity_relations WHERE from_entity = ?1 OR to_entity = ?1",
                    params![entity_id],
                )
                .context("failed to delete entity relations")?;

            // Delete changelog entries for the entity itself
            self.conn
                .execute(
                    "DELETE FROM changelog WHERE target_id = ?1",
                    params![entity_id],
                )
                .context("failed to delete changelog entries")?;

            // Delete the entity itself
            self.conn
                .execute("DELETE FROM entities WHERE id = ?1", params![entity_id])
                .context("failed to delete entity")?;

            Ok(())
        })?;
        Ok(true)
    }

    /// Find entities whose qualified name ends with `::{short_name}`.
    /// Used for fuzzy resolution when exact match fails.
    pub(super) fn find_entities_by_suffix(&self, short_name: &str) -> Result<Vec<Entity>> {
        let pattern = format!("%::{}", short_name);
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, qualified_name, kind, origin, metrics_json, created_at \
                 FROM entities WHERE qualified_name LIKE ?1 \
                 OR qualified_name = ?2 \
                 ORDER BY qualified_name",
            )
            .context("failed to prepare find_entities_by_suffix statement")?;
        let rows = stmt
            .query_map(params![pattern, short_name], entity_from_query_row)
            .context("failed to execute find_entities_by_suffix query")?;
        let mut results = Vec::new();
        for row in rows {
            let (id, name, kind, origin, metrics_json, ts) = row?;
            results.push(map_entity_row(id, name, kind, origin, metrics_json, ts)?);
        }
        Ok(results)
    }
}
