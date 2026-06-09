mod assertions;
mod changelog;
mod entities;
mod evidence;
mod helpers;
mod relations;
mod stats;
mod trait_impl;

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
}
