use std::fs;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::types::{
    Assertion, AssertionKind, AssertionRelation, AssertionRelationKind, AssertionStatus,
    ChangelogAction, ChangelogEntry, Entity, EntityKind, EntityOrigin, EntityRelation,
    EntityRelationKind, Evidence, ModelSnapshot, ModelStats, RelatedEntity, RelationDirection,
};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    qualified_name TEXT UNIQUE NOT NULL,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS assertions (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id),
    kind TEXT NOT NULL,
    claim TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    retraction_reason TEXT
);

CREATE TABLE IF NOT EXISTS evidences (
    id TEXT PRIMARY KEY,
    assertion_id TEXT NOT NULL REFERENCES assertions(id),
    source TEXT NOT NULL,
    detail TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS entity_relations (
    id TEXT PRIMARY KEY,
    from_entity TEXT NOT NULL REFERENCES entities(id),
    to_entity TEXT NOT NULL REFERENCES entities(id),
    kind TEXT NOT NULL,
    UNIQUE(from_entity, to_entity, kind)
);

CREATE TABLE IF NOT EXISTS assertion_relations (
    id TEXT PRIMARY KEY,
    from_assertion TEXT NOT NULL REFERENCES assertions(id),
    to_assertion TEXT NOT NULL REFERENCES assertions(id),
    kind TEXT NOT NULL,
    UNIQUE(from_assertion, to_assertion, kind)
);

CREATE TABLE IF NOT EXISTS changelog (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL,
    target_id TEXT NOT NULL,
    detail TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_assertions_entity ON assertions(entity_id);
CREATE INDEX IF NOT EXISTS idx_assertions_status ON assertions(status);
CREATE INDEX IF NOT EXISTS idx_evidences_assertion ON evidences(assertion_id);
CREATE INDEX IF NOT EXISTS idx_assertion_relations_from ON assertion_relations(from_assertion);
CREATE INDEX IF NOT EXISTS idx_assertion_relations_to ON assertion_relations(to_assertion);
CREATE INDEX IF NOT EXISTS idx_entity_relations_from ON entity_relations(from_entity);
CREATE INDEX IF NOT EXISTS idx_entity_relations_to ON entity_relations(to_entity);
CREATE INDEX IF NOT EXISTS idx_changelog_target ON changelog(target_id);
"#;

#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create db directory: {}", parent.display()))?;
        }

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

    pub fn vacuum_into(&self, target_path: &Path) -> Result<()> {
        let path_str = target_path.to_string_lossy();
        self.conn
            .execute_batch(&format!("VACUUM INTO '{}'", path_str.replace('\'', "''")))
            .context("failed to vacuum into target path")?;
        Ok(())
    }

    pub fn upsert_entity(
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
            created_at: Utc::now(),
        };

        self.conn
            .execute(
                "INSERT INTO entities (id, qualified_name, kind, origin, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entity.id,
                    entity.qualified_name,
                    entity.kind.to_string(),
                    entity.origin.to_string(),
                    to_ts(entity.created_at)
                ],
            )
            .with_context(|| format!("failed to insert entity: {qualified_name}"))?;

        Ok(entity)
    }

    /// Insert an entity with a pre-existing UUID (used during merge).
    /// Returns Ok(false) if an entity with the same qualified_name already exists.
    pub fn insert_entity(&self, entity: &Entity) -> Result<bool> {
        if self.get_entity_by_name(&entity.qualified_name)?.is_some() {
            return Ok(false);
        }
        self.conn
            .execute(
                "INSERT INTO entities (id, qualified_name, kind, origin, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entity.id,
                    entity.qualified_name,
                    entity.kind.to_string(),
                    entity.origin.to_string(),
                    to_ts(entity.created_at)
                ],
            )
            .with_context(|| format!("failed to insert entity: {}", entity.qualified_name))?;
        Ok(true)
    }
    /// Insert an assertion with a pre-existing UUID (used during merge).
    /// Unlike create_assertion, this does NOT auto-create evidence — evidence
    /// arrives as separate DiffItem::EvidenceAdded items.
    /// Returns Ok(false) if an assertion with the same id already exists.
    pub fn insert_assertion(&self, assertion: &Assertion) -> Result<bool> {
        if self.get_assertion(&assertion.id)?.is_some() {
            return Ok(false);
        }
        self.conn
            .execute(
                "INSERT INTO assertions \
                 (id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    assertion.id,
                    assertion.entity_id,
                    assertion.kind.to_string(),
                    assertion.claim,
                    assertion.status.to_string(),
                    to_ts(assertion.created_at),
                    to_ts(assertion.updated_at),
                    assertion.retraction_reason,
                ],
            )
            .with_context(|| format!("failed to insert assertion: {}", assertion.id))?;
        Ok(true)
    }

    pub fn get_entity(&self, id: &str) -> Result<Option<Entity>> {
        self.conn
            .query_row(
                "SELECT id, qualified_name, kind, origin, created_at FROM entities WHERE id = ?1",
                params![id],
                |row| {
                    let id: String = row.get(0)?;
                    let qualified_name: String = row.get(1)?;
                    let kind: String = row.get(2)?;
                    let origin: String = row.get(3)?;
                    let created_at: String = row.get(4)?;
                    Ok((id, qualified_name, kind, origin, created_at))
                },
            )
            .optional()
            .context("failed to fetch entity by id")?
            .map(|row| map_entity_row(row.0, row.1, row.2, row.3, row.4))
            .transpose()
    }

    pub fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>> {
        self.conn
            .query_row(
                "SELECT id, qualified_name, kind, origin, created_at FROM entities WHERE qualified_name = ?1",
                params![name],
                |row| {
                    let id: String = row.get(0)?;
                    let qualified_name: String = row.get(1)?;
                    let kind: String = row.get(2)?;
                    let origin: String = row.get(3)?;
                    let created_at: String = row.get(4)?;
                    Ok((id, qualified_name, kind, origin, created_at))
                },
            )
            .optional()
            .context("failed to fetch entity by name")?
            .map(|row| map_entity_row(row.0, row.1, row.2, row.3, row.4))
            .transpose()
    }

    pub fn list_entities(&self) -> Result<Vec<Entity>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, qualified_name, kind, origin, created_at FROM entities ORDER BY qualified_name",
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
            )?);
        }

        Ok(entities)
    }

    pub fn list_entities_with_counts(&self) -> Result<Vec<(Entity, usize)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.created_at, \
                 COUNT(a.id) AS assertion_count \
                 FROM entities e \
                 LEFT JOIN assertions a ON a.entity_id = e.id AND a.status = 'active' \
                 GROUP BY e.id \
                 ORDER BY assertion_count DESC, e.qualified_name",
            )
            .context("failed to prepare list_entities_with_counts statement")?;

        let mut rows = stmt
            .query([])
            .context("failed to query entities with counts")?;
        let mut result = Vec::new();
        while let Some(row) = rows
            .next()
            .context("failed to iterate entities with counts")?
        {
            let entity = map_entity_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )?;
            let count: usize = row.get::<_, i64>(5)? as usize;
            result.push((entity, count));
        }

        Ok(result)
    }

    pub fn create_assertion(
        &self,
        entity_id: &str,
        kind: AssertionKind,
        claim: &str,
        grounds: &str,
        depends_on: Option<&str>,
    ) -> Result<Assertion> {
        let now = Utc::now();
        let assertion = Assertion {
            id: Uuid::new_v4().to_string(),
            entity_id: entity_id.to_string(),
            kind,
            claim: claim.to_string(),
            status: AssertionStatus::Active,
            created_at: now,
            updated_at: now,
            retraction_reason: None,
        };

        self.conn
            .execute(
                "INSERT INTO assertions \
                 (id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![
                    assertion.id,
                    assertion.entity_id,
                    assertion.kind.to_string(),
                    assertion.claim,
                    assertion.status.to_string(),
                    to_ts(assertion.created_at),
                    to_ts(assertion.updated_at)
                ],
            )
            .with_context(|| format!("failed to insert assertion for entity: {entity_id}"))?;

        let (source, detail) = split_ground(grounds);
        self.create_evidence(&assertion.id, source, detail)?;

        if let Some(depends_on) = depends_on {
            self.add_assertion_dependency(&assertion.id, depends_on)?;
        }

        Ok(assertion)
    }

    pub fn get_assertion(&self, id: &str) -> Result<Option<Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason \
                 FROM assertions WHERE id = ?1",
            )
            .context("failed to prepare get_assertion statement")?;

        let mut rows = stmt
            .query(params![id])
            .context("failed to query assertion")?;

        match rows.next().context("failed to iterate assertion")? {
            Some(row) => Ok(Some(map_assertion_row(row)?)),
            None => Ok(None),
        }
    }

    pub fn resolve_assertion_id(&self, id: &str) -> Result<String> {
        // Try exact match first (full UUID)
        if let Some(assertion) = self.get_assertion(id)? {
            return Ok(assertion.id);
        }
        // Fallback: prefix match for short IDs
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM assertions WHERE id LIKE ?1 || '%' ORDER BY created_at")
            .context("failed to prepare resolve_assertion_id statement")?;
        let mut rows = stmt
            .query(params![id])
            .context("failed to query assertion by prefix")?;
        let mut matches = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate prefix matches")? {
            matches.push(row.get::<_, String>(0)?);
        }
        match matches.len() {
            0 => bail!("assertion not found: {id}"),
            1 => Ok(matches.into_iter().next().unwrap()),
            _ => bail!(
                "ambiguous short id '{}', matches {} assertions",
                id,
                matches.len()
            ),
        }
    }

    pub fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason \
                 FROM assertions WHERE entity_id = ?1 ORDER BY created_at",
            )
            .context("failed to prepare get_assertions_for_entity statement")?;

        let mut rows = stmt
            .query(params![entity_id])
            .context("failed to query assertions by entity")?;
        let mut assertions = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate assertions")? {
            assertions.push(map_assertion_row(row)?);
        }

        Ok(assertions)
    }

    pub fn list_assertions(&self) -> Result<Vec<Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason \
                 FROM assertions ORDER BY created_at",
            )
            .context("failed to prepare list_assertions statement")?;

        let mut rows = stmt.query([]).context("failed to query assertions")?;
        let mut assertions = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate assertions")? {
            assertions.push(map_assertion_row(row)?);
        }

        Ok(assertions)
    }

    pub fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()> {
        let now = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE assertions SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.to_string(), to_ts(now), id],
            )
            .with_context(|| format!("failed to update assertion status: {id}"))?;

        if changed == 0 {
            bail!("assertion not found: {id}");
        }

        Ok(())
    }

    pub fn retract_assertion(&self, id: &str, reason: &str) -> Result<()> {
        let now = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE assertions SET status = ?1, updated_at = ?2, retraction_reason = ?3 WHERE id = ?4",
                params![AssertionStatus::Retracted.to_string(), to_ts(now), reason, id],
            )
            .with_context(|| format!("failed to retract assertion: {id}"))?;
        if changed == 0 {
            bail!("assertion not found: {id}");
        }
        Ok(())
    }

    pub fn create_evidence(
        &self,
        assertion_id: &str,
        source: &str,
        detail: &str,
    ) -> Result<Evidence> {
        let evidence = Evidence {
            id: Uuid::new_v4().to_string(),
            assertion_id: assertion_id.to_string(),
            source: source.to_string(),
            detail: detail.to_string(),
            created_at: Utc::now(),
        };

        self.conn
            .execute(
                "INSERT INTO evidences (id, assertion_id, source, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    evidence.id,
                    evidence.assertion_id,
                    evidence.source,
                    evidence.detail,
                    to_ts(evidence.created_at)
                ],
            )
            .with_context(|| format!("failed to insert evidence for assertion: {assertion_id}"))?;

        Ok(evidence)
    }

    /// Insert evidence with a pre-existing UUID (used during merge).
    /// Returns Ok(false) if evidence with the same id already exists.
    pub fn insert_evidence(&self, evidence: &Evidence) -> Result<bool> {
        if self.get_evidence(&evidence.id)?.is_some() {
            return Ok(false);
        }
        self.conn
            .execute(
                "INSERT INTO evidences (id, assertion_id, source, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    evidence.id,
                    evidence.assertion_id,
                    evidence.source,
                    evidence.detail,
                    to_ts(evidence.created_at)
                ],
            )
            .with_context(|| format!("failed to insert evidence: {}", evidence.id))?;
        Ok(true)
    }

    pub fn get_evidence(&self, id: &str) -> Result<Option<Evidence>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, assertion_id, source, detail, created_at FROM evidences WHERE id = ?1",
            )
            .context("failed to prepare get_evidence statement")?;

        let mut rows = stmt
            .query(params![id])
            .context("failed to query evidence")?;

        match rows.next().context("failed to iterate evidence")? {
            Some(row) => Ok(Some(map_evidence_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )?)),
            None => Ok(None),
        }
    }

    pub fn get_evidence_for_assertion(&self, assertion_id: &str) -> Result<Vec<Evidence>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, assertion_id, source, detail, created_at \
                 FROM evidences WHERE assertion_id = ?1 ORDER BY created_at",
            )
            .context("failed to prepare get_evidence_for_assertion statement")?;

        let mut rows = stmt
            .query(params![assertion_id])
            .context("failed to query evidences")?;

        let mut evidences = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate evidences")? {
            evidences.push(map_evidence_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )?);
        }

        Ok(evidences)
    }

    pub fn list_evidences(&self) -> Result<Vec<Evidence>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, assertion_id, source, detail, created_at FROM evidences ORDER BY created_at")
            .context("failed to prepare list_evidences statement")?;

        let mut rows = stmt.query([]).context("failed to query evidences")?;
        let mut evidences = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate evidences")? {
            evidences.push(map_evidence_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )?);
        }

        Ok(evidences)
    }

    pub fn add_entity_relation(
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

    pub fn list_entity_relations(&self) -> Result<Vec<EntityRelation>> {
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

    pub fn add_assertion_dependency(&self, from_assertion: &str, to_assertion: &str) -> Result<()> {
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

    pub fn list_assertion_relations(&self) -> Result<Vec<AssertionRelation>> {
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

    pub fn get_dependents(&self, assertion_id: &str) -> Result<Vec<Assertion>> {
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

    pub fn get_dependencies(&self, assertion_id: &str) -> Result<Vec<Assertion>> {
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

    pub fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>> {
        let mut related = Vec::new();

        let mut out_stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.created_at, r.kind
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
            let relation_kind: String = row.get(5)?;
            related.push(RelatedEntity {
                entity: map_entity_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )?,
                kind: EntityRelationKind::from_str(&relation_kind)
                    .map_err(|_| anyhow!("invalid entity relation kind in db: {relation_kind}"))?,
                direction: RelationDirection::Outgoing,
            });
        }

        let mut in_stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.created_at, r.kind
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
            let relation_kind: String = row.get(5)?;
            related.push(RelatedEntity {
                entity: map_entity_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                )?,
                kind: EntityRelationKind::from_str(&relation_kind)
                    .map_err(|_| anyhow!("invalid entity relation kind in db: {relation_kind}"))?,
                direction: RelationDirection::Incoming,
            });
        }

        Ok(related)
    }

    pub fn get_impact_neighbors(&self, entity_id: &str) -> Result<Vec<Entity>> {
        // For contains: forward — children are impacted when parent changes
        // For uses/calls: reverse — dependents are impacted when dependency changes
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.qualified_name, e.kind, e.origin, e.created_at
                 FROM entity_relations r
                 JOIN entities e ON e.id = CASE
                     WHEN r.kind = 'contains' THEN r.to_entity
                     ELSE r.from_entity
                 END
                 WHERE CASE
                     WHEN r.kind = 'contains' THEN r.from_entity = ?1
                     ELSE r.to_entity = ?1
                 END
                 ORDER BY e.qualified_name",
            )
            .context("failed to prepare get_impact_neighbors statement")?;
        let mut rows = stmt
            .query(params![entity_id])
            .context("failed to query impact neighbors")?;

        let mut entities = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate impact neighbors")? {
            entities.push(map_entity_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )?);
        }
        Ok(entities)
    }

    /// Returns the qualified names of all entities that were created by automated scanning
    /// (origin = "scan"). Used by verify --scan to detect stale entities.
    pub fn get_scanned_entity_names(&self) -> Result<Vec<String>> {
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

    pub fn get_assertions_for_entities(&self, entity_ids: &[String]) -> Result<Vec<Assertion>> {
        let mut result = Vec::new();
        for entity_id in entity_ids {
            result.extend(self.get_assertions_for_entity(entity_id)?);
        }
        Ok(result)
    }

    pub fn list_changelog_entries(&self) -> Result<Vec<ChangelogEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, action, target_id, detail, timestamp FROM changelog ORDER BY timestamp",
            )
            .context("failed to prepare list_changelog_entries statement")?;
        let mut rows = stmt.query([]).context("failed to query changelog")?;

        let mut entries = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate changelog")? {
            let action: String = row.get(1)?;
            entries.push(ChangelogEntry {
                id: row.get(0)?,
                action: ChangelogAction::from_str(&action)
                    .map_err(|_| anyhow!("invalid changelog action in db: {action}"))?,
                target_id: row.get(2)?,
                detail: row.get(3)?,
                timestamp: parse_ts(&row.get::<_, String>(4)?)?,
            });
        }

        Ok(entries)
    }

    pub fn append_changelog(
        &self,
        action: ChangelogAction,
        target_id: &str,
        detail: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO changelog (id, action, target_id, detail, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    Uuid::new_v4().to_string(),
                    action.to_string(),
                    target_id,
                    detail,
                    to_ts(Utc::now())
                ],
            )
            .context("failed to append changelog entry")?;
        Ok(())
    }

    pub fn count_evidence_for_assertion(&self, assertion_id: &str) -> Result<u64> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM evidences WHERE assertion_id = ?1",
                params![assertion_id],
                |row| row.get(0),
            )
            .context("failed to count evidence")?;
        Ok(count as u64)
    }

    pub fn count_relations_for_entity(&self, entity_id: &str) -> Result<u64> {
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

    pub fn stats(&self) -> Result<ModelStats> {
        let entities: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .context("failed to count entities")?;
        let assertions: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM assertions", [], |row| row.get(0))
            .context("failed to count assertions")?;
        let active_assertions: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM assertions WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .context("failed to count active assertions")?;
        let uncertain_assertions: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM assertions WHERE status = 'uncertain'",
                [],
                |row| row.get(0),
            )
            .context("failed to count uncertain assertions")?;
        let retracted_assertions: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM assertions WHERE status = 'retracted'",
                [],
                |row| row.get(0),
            )
            .context("failed to count retracted assertions")?;
        let evidences: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM evidences", [], |row| row.get(0))
            .context("failed to count evidences")?;
        let corrections: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM assertions WHERE kind = 'correction'",
                [],
                |row| row.get(0),
            )
            .context("failed to count corrections")?;

        Ok(ModelStats {
            entities: entities as u64,
            assertions: assertions as u64,
            active_assertions: active_assertions as u64,
            uncertain_assertions: uncertain_assertions as u64,
            retracted_assertions: retracted_assertions as u64,
            evidences: evidences as u64,
            corrections: corrections as u64,
        })
    }

    /// Delete an entity and all associated data (assertions, evidence, relations, changelog).
    /// This is a destructive operation — all cross-references are removed.
    /// Returns Ok(false) if the entity does not exist.
    pub fn delete_entity(&self, qualified_name: &str) -> Result<bool> {
        let entity = match self.get_entity_by_name(qualified_name)? {
            Some(e) => e,
            None => return Ok(false),
        };
        let entity_id = &entity.id;

        // Get all assertion IDs for the entity
        let assertions = self.get_assertions_for_entity(entity_id)?;
        let assertion_ids: Vec<String> = assertions.iter().map(|a| a.id.clone()).collect();

        self.transaction(|| {
            // Delete evidence for all assertions
            for aid in &assertion_ids {
                self.conn
                    .execute("DELETE FROM evidences WHERE assertion_id = ?1", params![aid])
                    .with_context(|| format!("failed to delete evidence for assertion: {aid}"))?;
            }

            // Delete assertion relations involving these assertions
            for aid in &assertion_ids {
                self.conn
                    .execute(
                        "DELETE FROM assertion_relations WHERE from_assertion = ?1 OR to_assertion = ?1",
                        params![aid],
                    )
                    .with_context(|| format!("failed to delete assertion relations for: {aid}"))?;
            }

            // Delete assertions
            for aid in &assertion_ids {
                self.conn
                    .execute("DELETE FROM assertions WHERE id = ?1", params![aid])
                    .with_context(|| format!("failed to delete assertion: {aid}"))?;
            }

            // Delete entity relations
            self.conn
                .execute(
                    "DELETE FROM entity_relations WHERE from_entity = ?1 OR to_entity = ?1",
                    params![entity_id],
                )
                .context("failed to delete entity relations")?;

            // Delete changelog entries referencing this entity or its assertions
            self.conn
                .execute(
                    "DELETE FROM changelog WHERE target_id = ?1",
                    params![entity_id],
                )
                .context("failed to delete changelog entries")?;
            for aid in &assertion_ids {
                self.conn
                    .execute("DELETE FROM changelog WHERE target_id = ?1", params![aid])
                    .with_context(|| format!("failed to delete changelog for assertion: {aid}"))?;
            }

            // Delete the entity itself
            self.conn
                .execute("DELETE FROM entities WHERE id = ?1", params![entity_id])
                .context("failed to delete entity")?;

            Ok(())
        })?;

        Ok(true)
    }

    /// Snapshot the entire model state under a BEGIN/COMMIT for consistency.
    pub fn snapshot(&self) -> Result<ModelSnapshot> {
        // Execute all reads inside a transaction for snapshot isolation.
        // All six list methods run before checking errors, then we commit
        // (which is a no-op for reads) and propagate any failures.
        self.conn.execute_batch("BEGIN")?;
        let entities = self.list_entities();
        let assertions = self.list_assertions();
        let evidences = self.list_evidences();
        let entity_relations = self.list_entity_relations();
        let assertion_relations = self.list_assertion_relations();
        let changelog = self.list_changelog_entries();
        self.conn.execute_batch("COMMIT")?;

        Ok(ModelSnapshot {
            entities: entities?,
            assertions: assertions?,
            evidences: evidences?,
            entity_relations: entity_relations?,
            assertion_relations: assertion_relations?,
            changelog: changelog?,
        })
    }
}

fn split_ground(ground: &str) -> (&str, &str) {
    match ground.split_once(':') {
        Some((source, detail)) if !source.is_empty() && !detail.is_empty() => (source, detail),
        _ => ("note", ground),
    }
}

fn map_entity_row(
    id: String,
    qualified_name: String,
    kind: String,
    origin: String,
    created_at: String,
) -> Result<Entity> {
    Ok(Entity {
        id,
        qualified_name,
        kind: EntityKind::from_str(&kind)
            .map_err(|_| anyhow!("invalid entity kind in db: {kind}"))?,
        origin: EntityOrigin::from_str(&origin)
            .map_err(|_| anyhow!("invalid entity origin in db: {origin}"))?,
        created_at: parse_ts(&created_at)?,
    })
}

fn map_assertion_row(row: &rusqlite::Row<'_>) -> Result<Assertion> {
    let kind: String = row.get(2)?;
    let status: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    let updated_at: String = row.get(6)?;
    Ok(Assertion {
        id: row.get(0)?,
        entity_id: row.get(1)?,
        kind: AssertionKind::from_str(&kind)
            .map_err(|_| anyhow!("invalid assertion kind in db: {kind}"))?,
        claim: row.get(3)?,
        status: AssertionStatus::from_str(&status)
            .map_err(|_| anyhow!("invalid assertion status in db: {status}"))?,
        created_at: parse_ts(&created_at)?,
        updated_at: parse_ts(&updated_at)?,
        retraction_reason: row.get(7)?,
    })
}

fn map_evidence_row(
    id: String,
    assertion_id: String,
    source: String,
    detail: String,
    created_at: String,
) -> Result<Evidence> {
    Ok(Evidence {
        id,
        assertion_id,
        source,
        detail,
        created_at: parse_ts(&created_at)?,
    })
}

fn parse_ts(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("invalid timestamp in db: {value}"))
}

fn to_ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn upsert_and_fetch_entity() -> Result<()> {
        let tmp = tempdir()?;
        let db_path = tmp.path().join("cog.db");
        let store = Store::open(&db_path)?;

        let created =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        let fetched = store
            .get_entity_by_name("auth::login")?
            .ok_or_else(|| anyhow!("missing entity"))?;

        assert_eq!(created.id, fetched.id);
        assert_eq!(created.qualified_name, fetched.qualified_name);
        Ok(())
    }

    #[test]
    fn create_assertion_with_evidence_and_dependency() -> Result<()> {
        let tmp = tempdir()?;
        let db_path = tmp.path().join("cog.db");
        let store = Store::open(&db_path)?;

        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        let base = store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns option token",
            "code:src/auth.rs:10",
            None,
        )?;

        let dependent = store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "none means failure",
            "test:test_login_fail",
            Some(&base.id),
        )?;

        let evidences = store.get_evidence_for_assertion(&dependent.id)?;
        assert_eq!(evidences.len(), 1);
        let dependencies = store.get_dependencies(&dependent.id)?;
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].id, base.id);

        Ok(())
    }
}
