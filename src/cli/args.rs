use std::path::PathBuf;

use clap::Args;

use crate::domain::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind, ExportFormat};

#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Entity qualified name (e.g. auth::login)
    pub entity: String,
    /// Show all assertions including retracted
    #[arg(long)]
    pub all: bool,
    /// Compact mode — one assertion per line, no evidence or relations
    #[arg(long)]
    pub compact: bool,
}

#[derive(Debug, Args)]
pub struct ImpactArgs {
    /// Entity qualified name (e.g. auth::login)
    pub entity: String,
}

#[derive(Debug, Args)]
pub struct TraceArgs {
    /// Entity qualified name (e.g. auth::login)
    pub entity: String,
}

#[derive(Debug, Args)]
pub struct IndexArgs {
    /// Filter by entity kind (module, function, type, field, method)
    #[arg(long)]
    pub kind: Option<EntityKind>,
    /// Filter by origin (manual, scan)
    #[arg(long)]
    pub origin: Option<EntityOrigin>,
    /// Filter by qualified name prefix (e.g. "auth::")
    #[arg(long)]
    pub prefix: Option<String>,
    /// Full listing (restore old behavior, bypass summary)
    #[arg(long)]
    pub verbose: bool,
    /// Only show entities without assertions
    #[arg(long)]
    pub uncovered: bool,
}

#[derive(Debug, Args)]
pub struct AssertArgs {
    /// Entity qualified name (e.g. auth::login)
    pub entity: String,
    /// Kind of assertion
    #[arg(long)]
    pub kind: AssertionKind,
    /// The knowledge claim in natural language
    #[arg(long)]
    pub claim: String,
    /// Evidence or reasoning supporting this claim
    #[arg(long)]
    pub grounds: String,
    /// ID of another assertion this depends on
    #[arg(long)]
    pub depends_on: Option<String>,
}

#[derive(Debug, Args)]
pub struct RetractArgs {
    /// Short or full assertion ID to retract
    pub id: String,
    /// Reason for retraction
    #[arg(long)]
    pub reason: String,
}

#[derive(Debug, Args)]
pub struct DependArgs {
    /// Source entity qualified name
    pub entity_a: String,
    /// Target entity qualified name
    #[arg(long)]
    pub on: String,
    /// Kind of relationship
    #[arg(long)]
    pub kind: EntityRelationKind,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// Restrict checks to entities matching this prefix
    #[arg(long)]
    pub scope: Option<String>,
    /// Auto-delete isolated entities found during verification
    #[arg(long)]
    pub clean: bool,
    /// Compare model against actual code
    #[arg(long)]
    pub scan: bool,
    /// Path to scan (used with --scan; defaults to current directory)
    #[arg(long)]
    pub scan_path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Output format
    #[arg(long, default_value = "json")]
    pub format: ExportFormat,
}

#[derive(Debug, Args)]
pub struct StatsArgs;

#[derive(Debug, Args)]
pub struct DeleteEntityArgs {
    /// Entity qualified name to delete (cascades to assertions, evidence, relations)
    pub entity: String,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Initialize a new cognitive model at CWD before syncing
    #[arg(long)]
    pub init: bool,
    /// Only show what would be changed, don't write to database
    #[arg(long)]
    pub dry_run: bool,
    /// Only scan these languages (comma-separated, e.g. python,rust)
    #[arg(long)]
    pub lang: Option<String>,
}

#[derive(Debug, Args)]
pub struct NextArgs;
