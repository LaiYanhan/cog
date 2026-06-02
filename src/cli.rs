use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::command::{self, CommandOutput};
use crate::model::{AssertionKind, BranchManager, EntityRelationKind, ExportFormat, Store};

#[derive(Debug, Parser)]
#[command(name = "cog", about = "Cognitive model for coding agents")]
pub struct Cli {
    #[arg(long, env = "COG_DB")]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Query {
        entity: String,
        #[arg(long)]
        all: bool,
    },
    Impact {
        entity: String,
    },
    Trace {
        /// entity qualified name，如 auth::login
        entity: String,
    },
    Index,
    Assert {
        entity: String,
        #[arg(long)]
        kind: AssertionKind,
        #[arg(long)]
        claim: String,
        #[arg(long)]
        grounds: String,
        #[arg(long)]
        depends_on: Option<String>,
    },
    Retract {
        id: String,
        #[arg(long)]
        reason: String,
    },
    Depend {
        entity_a: String,
        #[arg(long)]
        on: String,
        #[arg(long)]
        kind: EntityRelationKind,
    },
    Verify {
        #[arg(long)]
        scope: Option<String>,
        /// Auto-delete isolated entities found during verification
        #[arg(long)]
        clean: bool,
    },
    Export {
        #[arg(long, default_value = "json")]
        format: ExportFormat,
    },
    Stats,
    DeleteEntity {
        /// Qualified entity name to delete (cascades to assertions, evidence, relations)
        entity: String,
    },
    Branch {
        #[command(subcommand)]
        action: BranchAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum BranchAction {
    Create {
        #[arg(long)]
        name: Option<String>,
    },
    List,
    Switch {
        name: String,
    },
    Diff {
        name: String,
        #[arg(long)]
        item: Option<usize>,
    },
    Merge {
        name: String,
        #[arg(long)]
        apply: Option<usize>,
        #[arg(long)]
        reject: Option<usize>,
        #[arg(long)]
        apply_all: bool,
    },
    Drop {
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
            Commands::Index => command::index_cmd::execute(store),
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
            Commands::Verify { scope, clean } => {
                command::verify::execute(store, scope.as_deref(), *clean)
            }
            Commands::Export { format } => command::export::execute(store, *format),
            Commands::Stats => command::stats::execute(store),
            Commands::DeleteEntity { entity } => command::entity_cmd::execute(store, entity),
            Commands::Branch { action } => {
                let mgr = BranchManager::new(&self.db_path());
                command::branch_cmd::execute(store, &mgr, action)
            }
        }
    }
}
