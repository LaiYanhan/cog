use anyhow::{Context, Result, bail};
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::domain::grounds::Grounds;
use crate::domain::{Assertion, AssertionKind, AssertionStatus};

use super::SqliteRepository;
use super::helpers::*;

impl SqliteRepository {
    pub(super) fn create_assertion(
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

        let grounds = Grounds::parse(grounds);
        self.create_evidence(&assertion.id, &grounds.source, &grounds.detail)?;

        if let Some(depends_on) = depends_on {
            self.add_assertion_dependency(&assertion.id, depends_on)?;
        }

        Ok(assertion)
    }

    pub(super) fn get_assertion(&self, id: &str) -> Result<Option<Assertion>> {
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

    pub(super) fn resolve_assertion_id(&self, id: &str) -> Result<String> {
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

    pub(super) fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason \
                 FROM assertions WHERE entity_id = ?1 ORDER BY created_at DESC",
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

    pub(super) fn get_assertions_for_entities(
        &self,
        entity_ids: &[String],
    ) -> Result<Vec<Assertion>> {
        if entity_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = (0..entity_ids.len()).map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id, entity_id, kind, claim, status, created_at, updated_at, retraction_reason \
             FROM assertions WHERE entity_id IN ({}) ORDER BY created_at DESC",
            placeholders.join(", ")
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .context("failed to prepare get_assertions_for_entities statement")?;
        let params: Vec<&dyn rusqlite::types::ToSql> = entity_ids
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let mut rows = stmt
            .query(params.as_slice())
            .context("failed to query assertions by entity ids")?;
        let mut assertions = Vec::new();
        while let Some(row) = rows.next().context("failed to iterate assertions")? {
            assertions.push(map_assertion_row(row)?);
        }
        Ok(assertions)
    }

    pub(super) fn list_assertions(&self) -> Result<Vec<Assertion>> {
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

    pub(super) fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()> {
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

    pub(super) fn retract_assertion(&self, id: &str, reason: &str) -> Result<()> {
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
}
