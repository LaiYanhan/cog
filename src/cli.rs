use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::command::{self, CommandOutput};
use crate::model::{AssertionKind, EntityRelationKind, ExportFormat, Store};

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
    },
    Export {
        #[arg(long, default_value = "json")]
        format: ExportFormat,
    },
    Stats,
}

impl Cli {
    pub fn db_path(&self) -> PathBuf {
        self.db
            .clone()
            .unwrap_or_else(|| PathBuf::from(".cog/cog.db"))
    }

    pub fn run(&self, store: &Store) -> Result<CommandOutput> {
        match &self.command {
            Commands::Query { entity } => command::query::execute(store, entity),
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
            Commands::Verify { scope } => command::verify::execute(store, scope.as_deref()),
            Commands::Export { format } => command::export::execute(store, *format),
            Commands::Stats => command::stats::execute(store),
        }
    }
}
