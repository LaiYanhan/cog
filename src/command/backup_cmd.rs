use anyhow::Result;

use crate::backup::BackupManager;
use crate::command::CommandOutput;

pub fn create(
    repo: &dyn crate::repo::Repository,
    mgr: &BackupManager,
    name: Option<String>,
) -> Result<CommandOutput> {
    let backup_name = name.unwrap_or_else(|| {
        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
        format!("backup-{ts}")
    });
    let path = mgr.create(repo, &backup_name)?;
    Ok(CommandOutput::success(format!(
        "backup created: {backup_name} ({})",
        path.display()
    )))
}

pub fn list(mgr: &BackupManager) -> Result<CommandOutput> {
    let backups = mgr.list()?;
    if backups.is_empty() {
        return Ok(CommandOutput::success("no backups found"));
    }
    let mut text = String::from("backups:\n");
    for name in &backups {
        text.push_str(&format!("  {name}\n"));
    }
    Ok(CommandOutput::success(text))
}

pub fn restore(mgr: &BackupManager, name: &str) -> Result<CommandOutput> {
    mgr.restore(name)?;
    Ok(CommandOutput::success(format!(
        "restored from backup: {name}"
    )))
}

pub fn drop(mgr: &BackupManager, name: &str) -> Result<CommandOutput> {
    mgr.drop(name)?;
    Ok(CommandOutput::success(format!("deleted backup: {name}")))
}
