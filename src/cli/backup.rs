use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum BackupAction {
    /// Create a full backup of the current model
    Create {
        /// Backup name
        #[arg(long)]
        name: Option<String>,
    },
    /// List all backups
    List,
    /// Restore from a backup (overwrites current model)
    Restore {
        /// Backup name to restore
        name: String,
    },
    /// Delete a backup
    Drop {
        /// Backup name to delete
        name: String,
    },
}
