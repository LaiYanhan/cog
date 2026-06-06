use std::str::FromStr;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::domain::{ChangelogAction, ChangelogEntry};

use super::SqliteRepository;
use super::helpers::*;

impl SqliteRepository {
    pub(super) fn list_changelog_entries(&self) -> Result<Vec<ChangelogEntry>> {
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

    pub(super) fn append_changelog(
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
}
