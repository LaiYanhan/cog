use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::command::{self, CommandOutput};
use crate::model::{
    AssertionKind, BranchManager, EntityKind, EntityOrigin, EntityRelationKind, ExportFormat, Store,
};

#[derive(Debug, Parser)]
#[command(name = "cog", about = "Cognitive model for coding agents", version)]
pub struct Cli {
    /// Path to the cognitive model database
    #[arg(long, env = "COG_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Show assertions and relations for an entity
    Query {
        /// Entity qualified name (e.g. auth::login)
        entity: String,
        /// Show all assertions including retracted
        #[arg(long)]
        all: bool,
    },
    /// Trace downstream impact of retracting an entity or assertion
    Impact {
        /// Entity qualified name (e.g. auth::login)
        entity: String,
    },
    /// Trace dependency chain leading to an entity
    Trace {
        /// Entity qualified name (e.g. auth::login)
        entity: String,
    },
    /// List all entities in the model
    Index {
        /// Filter by entity kind (module, function, type, field, method)
        #[arg(long)]
        kind: Option<EntityKind>,
        /// Filter by origin (manual, scan)
        #[arg(long)]
        origin: Option<EntityOrigin>,
        /// Filter by qualified name prefix (e.g. "auth::")
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Record a knowledge claim (assertion) about an entity
    Assert {
        /// Entity qualified name (e.g. auth::login)
        entity: String,
        /// Kind of assertion
        #[arg(long)]
        kind: AssertionKind,
        /// The knowledge claim in natural language
        #[arg(long)]
        claim: String,
        /// Evidence or reasoning supporting this claim
        #[arg(long)]
        grounds: String,
        /// ID of another assertion this depends on
        #[arg(long)]
        depends_on: Option<String>,
    },
    /// Retract (deprecate) an assertion by ID
    Retract {
        /// Short or full assertion ID to retract
        id: String,
        /// Reason for retraction
        #[arg(long)]
        reason: String,
    },
    /// Record a structural relationship between two entities
    Depend {
        /// Source entity qualified name
        entity_a: String,
        /// Target entity qualified name
        #[arg(long)]
        on: String,
        /// Kind of relationship
        #[arg(long)]
        kind: EntityRelationKind,
    },
    /// Check structural consistency of the model
    Verify {
        /// Restrict checks to entities matching this prefix
        #[arg(long)]
        scope: Option<String>,
        /// Auto-delete isolated entities found during verification
        #[arg(long)]
        clean: bool,
        /// Compare model against actual code
        #[arg(long)]
        scan: bool,
        /// Path to scan (used with --scan; defaults to current directory)
        #[arg(long)]
        scan_path: Option<PathBuf>,
    },
    /// Export the model to a file
    Export {
        /// Output format
        #[arg(long, default_value = "json")]
        format: ExportFormat,
    },
    /// Show model statistics
    Stats,
    /// Delete an entity and all its assertions, evidence, and relations
    DeleteEntity {
        /// Entity qualified name to delete (cascades to assertions, evidence, relations)
        entity: String,
    },
    /// Manage model branches for speculative changes
    Branch {
        #[command(subcommand)]
        action: BranchAction,
    },
    /// Scan a codebase and populate the model with structural entities
    Init {
        /// Path to scan (defaults to current directory)
        path: Option<PathBuf>,
        /// Only show what would be created, don't write to database
        #[arg(long)]
        dry_run: bool,
        /// Maximum directory traversal depth
        #[arg(long)]
        depth: Option<usize>,
        /// Only scan these languages (comma-separated, e.g. python,rust)
        #[arg(long)]
        lang: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum BranchAction {
    /// Create a new branch for speculative changes
    Create {
        /// Branch name (auto-generated if omitted)
        #[arg(long)]
        name: Option<String>,
    },
    /// List all branches
    List,
    /// Switch to a branch
    Switch {
        /// Branch name
        name: String,
    },
    /// Compare a branch against main
    Diff {
        /// Branch name
        name: String,
        /// Show detail for a specific diff item (1-indexed)
        #[arg(long)]
        item: Option<usize>,
    },
    /// Merge a branch back into main
    Merge {
        /// Branch name
        name: String,
        /// Apply a specific diff item by index
        #[arg(long)]
        apply: Option<usize>,
        /// Reject a specific diff item by index
        #[arg(long)]
        reject: Option<usize>,
        /// Apply all remaining diff items
        #[arg(long)]
        apply_all: bool,
    },
    /// Delete a branch
    Drop {
        /// Branch name
        name: String,
    },
}

impl Cli {
    pub fn db_path(&self) -> PathBuf {
        self.db
            .clone()
            .unwrap_or_else(|| PathBuf::from(".cog/cog.db"))
    }

    pub fn run(&self, store: &Store) -> Result<CommandOutput> {
        match &self.command {
            Commands::Query { entity, all } => command::query::execute(store, entity, *all),
            Commands::Impact { entity } => command::impact::execute(store, entity),
            Commands::Trace { entity } => command::trace::execute(store, entity),
            Commands::Index { kind, origin, prefix } => {
                command::index_cmd::execute(store, *kind, *origin, prefix.as_deref())
            }
            Commands::Assert {
                entity,
                kind,
                claim,
                grounds,
                depends_on,
            } => command::assert_cmd::execute(
                store,
                entity,
                *kind,
                claim,
                grounds,
                depends_on.as_deref(),
            ),
            Commands::Retract { id, reason } => command::retract::execute(store, id, reason),
            Commands::Depend { entity_a, on, kind } => {
                command::depend::execute(store, entity_a, on, *kind)
            }
            Commands::Verify {
                scope,
                clean,
                scan,
                scan_path,
            } => {
                let resolved = if *scan {
                    Some(
                        scan_path
                            .as_deref()
                            .unwrap_or_else(|| std::path::Path::new(".")),
                    )
                } else {
                    None
                };
                command::verify::execute(store, scope.as_deref(), *clean, resolved)
            }
            Commands::Export { format } => command::export::execute(store, *format),
            Commands::Stats => command::stats::execute(store),
            Commands::DeleteEntity { entity } => command::entity_cmd::execute(store, entity),
            Commands::Branch { action } => {
                let mgr = BranchManager::new(&self.db_path());
                command::branch_cmd::execute(store, &mgr, action)
            }
            Commands::Init {
                path,
                dry_run,
                depth,
                lang,
            } => {
                let scan_path = path
                    .as_deref()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                let lang_list: Option<Vec<String>> = lang
                    .as_ref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
                command::init_cmd::execute(store, &scan_path, *dry_run, *depth, lang_list)
            }
        }
    }
}
