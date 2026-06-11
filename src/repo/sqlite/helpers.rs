use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::Row;

use crate::domain::{
    Assertion, AssertionKind, AssertionStatus, Entity, EntityKind, EntityOrigin, Evidence,
};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    qualified_name TEXT UNIQUE NOT NULL,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'manual',
    metrics_json TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS assertions (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id),
    kind TEXT NOT NULL,
    claim TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'retracted', 'uncertain')),
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

/// Extracts the 6 standard entity columns from a `query_row` callback into a tuple,
/// then maps through `map_entity_row`. Shared by `get_entity` and `get_entity_by_name`.
pub(crate) fn entity_from_query_row(
    row: &Row<'_>,
) -> rusqlite::Result<(String, String, String, String, Option<String>, String)> {
    let id: String = row.get(0)?;
    let qualified_name: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let origin: String = row.get(3)?;
    let metrics_json: Option<String> = row.get(4)?;
    let created_at: String = row.get(5)?;
    Ok((id, qualified_name, kind, origin, metrics_json, created_at))
}

pub(crate) fn map_entity_row(
    id: String,
    qualified_name: String,
    kind: String,
    origin: String,
    metrics_json: Option<String>,
    created_at: String,
) -> Result<Entity> {
    let metrics: crate::domain::metrics::EntityMetrics = metrics_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    Ok(Entity {
        id,
        qualified_name,
        kind: EntityKind::from_str(&kind)
            .map_err(|_| anyhow!("invalid entity kind in db: {kind}"))?,
        origin: EntityOrigin::from_str(&origin)
            .map_err(|_| anyhow!("invalid entity origin in db: {origin}"))?,
        metrics,
        created_at: parse_ts(&created_at)?,
    })
}

pub(crate) fn map_assertion_row(row: &Row<'_>) -> Result<Assertion> {
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

pub(crate) fn map_evidence_row(
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

pub(crate) fn parse_ts(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("invalid timestamp in db: {value}"))
}

pub(crate) fn to_ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}
