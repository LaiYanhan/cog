use anyhow::{Context, Result, anyhow};
use std::path::PathBuf;

pub struct BackupManager {
    db_path: PathBuf,
}

impl BackupManager {
    pub fn new(db_path: &std::path::Path) -> Self {
        Self {
            db_path: db_path.to_path_buf(),
        }
    }

    /// Create a full backup of the current database.
    /// Returns the backup file path.
    pub fn create(&self, repo: &dyn crate::repo::Repository, name: &str) -> Result<PathBuf> {
        let backup_dir = self
            .db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("backups");
        std::fs::create_dir_all(&backup_dir).context("failed to create backups directory")?;

        let backup_path = backup_dir.join(format!("{name}.db"));
        if backup_path.exists() {
            return Err(anyhow!("backup already exists: {name}"));
        }

        repo.vacuum_into(&backup_path)
            .with_context(|| format!("failed to create backup '{}': vacuum into failed", name))?;

        Ok(backup_path)
    }

    /// List all available backups by name.
    pub fn list(&self) -> Result<Vec<String>> {
        let backup_dir = self
            .db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("backups");
        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();
        for entry in std::fs::read_dir(&backup_dir)? {
            let entry = entry?;
            if let Some(ext) = entry.path().extension()
                && ext == "db"
                && let Some(stem) = entry.path().file_stem()
            {
                backups.push(stem.to_string_lossy().to_string());
            }
        }
        backups.sort();
        Ok(backups)
    }

    /// Restore from a backup. The caller must checkpoint the WAL on the live
    /// connection *before* calling this — the file copy is only safe once the
    /// WAL is flushed. After the copy, stale WAL/SHM sidecar files are removed
    /// so a fresh connection won't try to replay an outdated WAL.
    pub fn restore(&self, name: &str) -> Result<()> {
        let backup_dir = self
            .db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("backups");
        let backup_path = backup_dir.join(format!("{name}.db"));

        if !backup_path.exists() {
            return Err(anyhow!("backup not found: {name}"));
        }

        std::fs::copy(&backup_path, &self.db_path)
            .with_context(|| format!("failed to restore backup {name}"))?;

        // Remove stale WAL/SHM sidecars (SQLite uses <db>-wal and <db>-shm).
        for suffix in &["-wal", "-shm"] {
            let mut sidecar = self.db_path.as_os_str().to_owned();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(&sidecar);
        }

        Ok(())
    }

    /// Delete a backup file.
    pub fn drop(&self, name: &str) -> Result<()> {
        let backup_dir = self
            .db_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("backups");
        let backup_path = backup_dir.join(format!("{name}.db"));

        if !backup_path.exists() {
            return Err(anyhow!("backup not found: {name}"));
        }

        std::fs::remove_file(&backup_path)
            .with_context(|| format!("failed to delete backup {name}"))?;

        Ok(())
    }
}
