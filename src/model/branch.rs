use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use crate::model::Store;

const BRANCHES_DIR: &str = "branches";
const MAIN_BACKUP: &str = "_main_backup";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BranchInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<DateTime<Utc>>,
}

pub struct BranchManager {
    db_path: PathBuf,
    branches_dir: PathBuf,
}

impl BranchManager {
    pub fn new(db_path: &Path) -> Self {
        let branches_dir = db_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(BRANCHES_DIR);
        Self {
            db_path: db_path.to_path_buf(),
            branches_dir,
        }
    }

    pub fn active_branch(&self) -> Option<String> {
        let marker = self.branches_dir.join(".active_branch");
        fs::read_to_string(&marker).ok().filter(|s| !s.is_empty())
    }

    fn set_active_branch(&self, name: Option<&str>) {
        let marker = self.branches_dir.join(".active_branch");
        if let Some(name) = name {
            let _ = fs::write(&marker, name);
        } else {
            let _ = fs::remove_file(&marker);
        }
    }
    pub fn create(&self, store: &Store, name: &str) -> Result<BranchInfo> {
        validate_branch_name(name)?;

        fs::create_dir_all(&self.branches_dir).with_context(|| {
            format!(
                "failed to create branches directory: {}",
                self.branches_dir.display()
            )
        })?;

        let branch_path = self.branch_path(name);
        if branch_path.exists() {
            bail!("branch already exists: {name}");
        }

        // Use VACUUM INTO for a self-contained snapshot (no WAL/SHM dependency)
        store
            .vacuum_into(&branch_path)
            .with_context(|| format!("failed to create branch '{}': vacuum into failed", name))?;

        let info = self.branch_info_from_path(name, &branch_path)?;
        Ok(info)
    }

    pub fn list(&self) -> Result<Vec<BranchInfo>> {
        if !self.branches_dir.exists() {
            return Ok(Vec::new());
        }

        let mut branches = Vec::new();
        let entries = fs::read_dir(&self.branches_dir).with_context(|| {
            format!(
                "failed to read branches directory: {}",
                self.branches_dir.display()
            )
        })?;

        for entry in entries {
            let entry = entry.context("failed to read directory entry")?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "db") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(unknown)");
                if stem == "_main_backup" {
                    continue;
                }
                if let Ok(info) = self.branch_info_from_path(stem, &path) {
                    branches.push(info);
                }
            }
        }

        branches.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(branches)
    }

    pub fn drop(&self, name: &str) -> Result<()> {
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch not found: {name}");
        }
        fs::remove_file(&path)
            .with_context(|| format!("failed to delete branch file: {}", path.display()))?;
        // Clean up WAL/SHM if present
        for ext in &["db-wal", "db-shm"] {
            let sidecar = path.with_extension(ext);
            if sidecar.exists() {
                let _ = fs::remove_file(&sidecar);
            }
        }
        Ok(())
    }

    pub fn load_branch_store(&self, name: &str) -> Result<Store> {
        let path = self.branch_path(name);
        if !path.exists() {
            bail!("branch not found: {name}");
        }
        Store::open(&path)
    }

    pub fn branch_path(&self, name: &str) -> PathBuf {
        self.branches_dir.join(format!("{name}.db"))
    }

    #[allow(dead_code)]
    pub fn is_on_branch(&self) -> Result<bool> {
        let backup = self.branch_path(MAIN_BACKUP);
        Ok(backup.exists())
    }

    pub fn switch_to_branch(&self, name: &str) -> Result<()> {
        let branch_path = self.branch_path(name);
        if !branch_path.exists() {
            bail!("branch not found: {name}");
        }

        let backup = self.branch_path(MAIN_BACKUP);
        if backup.exists() {
            bail!("already on a branch (main backup exists). Switch back to main first.");
        }

        // Move current DB to backup, move branch to main DB path
        fs::rename(&self.db_path, &backup).with_context(|| "failed to back up main DB")?;
        // Copy branch to main position (keep branch file intact)
        fs::copy(&branch_path, &self.db_path)
            .with_context(|| "failed to copy branch to main position")?;

        self.set_active_branch(Some(name));
        Ok(())
    }

    pub fn switch_to_main(&self, current_branch: Option<&str>) -> Result<()> {
        let backup = self.branch_path(MAIN_BACKUP);
        if !backup.exists() {
            bail!("not on a branch (no main backup found)");
        }

        // If a branch name is given, save current (modified) DB back to the branch file
        // before restoring the original main DB.
        if let Some(branch_name) = current_branch {
            let branch_path = self.branch_path(branch_name);
            if !branch_path.exists() {
                bail!("branch file not found: {branch_name}");
            }
            // Overwrite branch file with current state (includes user's edits)
            fs::copy(&self.db_path, &branch_path).with_context(|| {
                format!("failed to save branch state to {}", branch_path.display())
            })?;
        }

        // Restore original main DB
        fs::rename(&backup, &self.db_path).with_context(|| "failed to restore main DB")?;
        // Clear active branch marker since we're back on main
        self.set_active_branch(None);

        Ok(())
    }

    fn branch_info_from_path(&self, name: &str, path: &Path) -> Result<BranchInfo> {
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read branch file: {}", path.display()))?;
        let modified = metadata
            .modified()
            .ok()
            .map(|st: std::time::SystemTime| DateTime::<Utc>::from(st));

        Ok(BranchInfo {
            name: name.to_string(),
            path: path.to_path_buf(),
            size_bytes: metadata.len(),
            modified,
        })
    }
}

fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("branch name cannot be empty");
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        bail!("branch name cannot contain path separators or '..'");
    }
    if name == "_main" || name == "_main_backup" {
        bail!("'_main' and '_main_backup' are reserved branch names");
    }
    Ok(())
}
