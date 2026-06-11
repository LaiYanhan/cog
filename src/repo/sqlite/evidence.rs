use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::domain::Evidence;

use super::SqliteRepository;
use super::helpers::*;

impl SqliteRepository {
    pub(super) fn create_evidence(
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

    pub(super) fn get_evidence_for_assertion(&self, assertion_id: &str) -> Result<Vec<Evidence>> {
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

    /// Load evidences for multiple assertions at once.
    /// Returns all evidences grouped — caller partitions by assertion_id.
    pub(super) fn get_evidences_for_assertions(
        &self,
        assertion_ids: &[String],
    ) -> Result<Vec<Evidence>> {
        if assertion_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Use a temp table to avoid parameter limits.
        self.conn
            .execute_batch("CREATE TEMP TABLE IF NOT EXISTS _ev_target_ids(id TEXT PRIMARY KEY); DELETE FROM _ev_target_ids;")
            .context("failed to prepare temp table")?;
        {
            let mut ins = self
                .conn
                .prepare("INSERT OR IGNORE INTO _ev_target_ids VALUES (?1)")
                .context("failed to prepare temp insert")?;
            for id in assertion_ids {
                ins.execute(params![id])?;
            }
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.id, e.assertion_id, e.source, e.detail, e.created_at
                 FROM evidences e
                 WHERE EXISTS (SELECT 1 FROM _ev_target_ids WHERE id = e.assertion_id)
                 ORDER BY e.created_at",
            )
            .context("failed to prepare get_evidences_for_assertions statement")?;
        let mut rows = stmt.query([])?;
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

    pub(super) fn list_evidences(&self) -> Result<Vec<Evidence>> {
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
}
