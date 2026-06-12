use std::path::Path;

use anyhow::{Context, Result};

use crate::domain::ModelStats;

use super::SqliteRepository;

impl SqliteRepository {
    pub(super) fn stats(&self) -> Result<ModelStats> {
        let (entities, assertions, active, uncertain, retracted, evidences, corrections): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = self
            .conn
            .query_row(
                "SELECT \
                 (SELECT COUNT(*) FROM entities), \
                 (SELECT COUNT(*) FROM assertions), \
                 (SELECT COUNT(*) FROM assertions WHERE status = 'active'), \
                 (SELECT COUNT(*) FROM assertions WHERE status = 'uncertain'), \
                 (SELECT COUNT(*) FROM assertions WHERE status = 'retracted'), \
                 (SELECT COUNT(*) FROM evidences), \
                 (SELECT COUNT(*) FROM assertions WHERE kind = 'correction')",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .context("failed to compute stats")?;

        // Compute covered entities: entities with at least one assertion
        let covered: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT e.id) FROM entities e \
                 INNER JOIN assertions a ON a.entity_id = e.id",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(ModelStats {
            entities: entities as u64,
            assertions: assertions as u64,
            active_assertions: active as u64,
            uncertain_assertions: uncertain as u64,
            retracted_assertions: retracted as u64,
            evidences: evidences as u64,
            corrections: corrections as u64,
            covered_entities: covered as u64,
        })
    }

    pub(super) fn vacuum_into(&self, target_path: &Path) -> Result<()> {
        let path_str = target_path.to_string_lossy();
        self.conn
            .execute_batch(&format!("VACUUM INTO '{}'", path_str.replace('\'', "''")))
            .context("failed to vacuum into target path")?;
        Ok(())
    }
}
