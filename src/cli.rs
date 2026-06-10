mod args;
mod backup;
mod experiment;

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};

pub use args::*;
pub use backup::BackupAction;
pub use experiment::ExperimentAction;

use crate::backup::BackupManager;
use crate::command::{self, CommandOutput};
use crate::repo::SqliteRepository;
use crate::workflow::WorkflowState;
#[derive(Debug, Parser)]
#[command(name = "cog", about = "Cognitive model for coding agents", version)]
pub struct Cli {
    /// Path to the cognitive model database
    #[arg(long, env = "COG_DB")]
    db: Option<PathBuf>,

    /// Output format: text (default) or json
    #[arg(long, global = true, default_value = "text")]
    output: crate::format::OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show assertions and relations for an entity
    Query(QueryArgs),
    /// Trace downstream impact of retracting an entity or assertion
    Impact(ImpactArgs),
    /// Trace dependency chain leading to an entity
    Trace(TraceArgs),
    /// List all entities in the model
    Index(IndexArgs),
    /// Record a knowledge claim (assertion) about an entity
    Assert(AssertArgs),
    /// Retract (deprecate) an assertion by ID
    Retract(RetractArgs),
    /// Record a structural relationship between two entities
    Depend(DependArgs),
    /// Check structural consistency of the model
    Verify(VerifyArgs),
    /// Export the model to a file
    Export(ExportArgs),
    /// Show model statistics
    Stats(StatsArgs),
    /// Delete an entity and all its assertions, evidence, and relations
    DeleteEntity(DeleteEntityArgs),
    /// Run hypothesis experiments without modifying the real model
    Experiment {
        #[command(subcommand)]
        action: ExperimentAction,
    },
    /// Manage model backups (simplified snapshots without diff/merge)
    Backup {
        #[command(subcommand)]
        action: BackupAction,
    },
    /// Show suggested next actions based on current workflow state
    Next(NextArgs),
    /// Sync the cognitive model with the codebase (idempotent). Use --init to create a new model.
    Sync(SyncArgs),
}

impl Cli {
    /// Find an existing `.cog/cog.db` by walking up from CWD, or from the
    /// explicit `--db` path. Returns `None` if no existing DB is found.
    pub fn find_existing_db(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.db {
            return if p.exists() { Some(p.clone()) } else { None };
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join(".cog").join("cog.db");
            if candidate.exists() {
                return Some(candidate);
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        None
    }

    /// DB path for `sync --init`: always at `<CWD>/.cog/cog.db`.
    pub fn init_db_path(&self) -> PathBuf {
        if let Some(ref p) = self.db {
            return p.clone();
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        cwd.join(".cog").join("cog.db")
    }

    /// Resolve the DB path: existing DB found by walk-up, or explicit --db.
    /// Does NOT create directories — returns a path that may not exist yet.
    pub fn db_path(&self) -> PathBuf {
        self.find_existing_db().unwrap_or_else(|| {
            self.db
                .clone()
                .unwrap_or_else(|| PathBuf::from(".cog/cog.db"))
        })
    }

    /// Returns true if this is a `sync --init` command.
    pub fn is_sync_init(&self) -> bool {
        matches!(&self.command, Commands::Sync(args) if args.init)
    }

    /// Returns the explicit `--db` path if one was provided.
    pub fn explicit_db(&self) -> Option<&PathBuf> {
        self.db.as_ref()
    }

    pub fn run(&self, store: &SqliteRepository) -> Result<CommandOutput> {
        let cog_dir = self
            .db_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        // Ensure .cog/ dir exists for workflow state
        let _ = std::fs::create_dir_all(&cog_dir);
        let mut wf = WorkflowState::load(&cog_dir);

        let result = match &self.command {
            Commands::Query(args) => {
                let out = command::query::execute(
                    store,
                    &args.entity,
                    args.all,
                    args.compact,
                    self.output,
                )?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Impact(args) => {
                let out = command::impact::execute(store, &args.entity, self.output)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Trace(args) => {
                let out = command::trace::execute(store, &args.entity, self.output)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Index(args) => {
                let out = command::index_cmd::execute(
                    store,
                    args.kind,
                    args.origin,
                    args.prefix.as_deref(),
                    args.verbose,
                    args.uncovered,
                    self.output,
                )?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Assert(args) => {
                let out = command::assert_cmd::execute(
                    store,
                    command::assert_cmd::AssertInput {
                        entity: &args.entity,
                        kind: args.kind,
                        claim: &args.claim,
                        grounds: &args.grounds,
                        depends_on: args.depends_on.as_deref(),
                        replace: args.replace,
                        force: args.force,
                        output: self.output,
                    },
                )?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Retract(args) => {
                let out = command::retract::execute(store, &args.id, &args.reason, self.output)?;
                wf.transition_retract();
                Ok(out)
            }
            Commands::Depend(args) => {
                let out = command::depend::execute(
                    store,
                    &args.entity_a,
                    &args.on,
                    args.kind,
                    self.output,
                )?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Verify(args) => {
                let resolved = if args.scan {
                    Some(
                        args.scan_path
                            .as_deref()
                            .unwrap_or_else(|| std::path::Path::new(".")),
                    )
                } else {
                    None
                };
                let out = command::verify::execute(
                    store,
                    args.scope.as_deref(),
                    args.clean,
                    resolved,
                    self.output,
                )?;
                let passed = out.exit_code == 0;
                wf.transition_verify(passed);
                Ok(out)
            }
            Commands::Export(args) => {
                let out = command::export::execute(store, args.format)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Stats(_) => {
                let out = command::stats::execute(store, self.output)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::DeleteEntity(args) => {
                let out = command::entity_cmd::execute(store, &args.entity)?;
                // delete-entity doesn't change phase (design §5.1) — next `cog next`
                // may suggest verify due to the destructive operation.
                wf.transition_browse();
                Ok(out)
            }
            Commands::Experiment { action } => self.run_experiment(action, &cog_dir, store),
            Commands::Backup { action } => self.run_backup(action, store),
            Commands::Sync(args) => {
                let lang_list: Option<Vec<String>> = args
                    .lang
                    .as_ref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
                let db = self.db_path();
                let out =
                    command::sync_cmd::execute(store, &db, args.dry_run, lang_list, self.output)?;
                if out.exit_code == 0 && !args.dry_run {
                    if matches!(wf, WorkflowState::Uninit) {
                        wf.transition_init().ok();
                    } else {
                        wf.transition_sync(out.has_drift);
                    }
                }
                Ok(out)
            }
            Commands::Next(_) => command::next_cmd::execute(store, &wf, &cog_dir, self.output),
        };

        self.apply_workflow_then_save(&mut wf, &cog_dir, result)
    }

    fn run_experiment(
        &self,
        action: &ExperimentAction,
        cog_dir: &Path,
        store: &SqliteRepository,
    ) -> Result<CommandOutput> {
        use ExperimentAction::*;
        match action {
            Try {
                entity,
                kind,
                claim,
                grounds,
                desc,
                depends_on,
            } => command::experiment_cmd::try_experiment(
                store,
                &command::experiment_cmd::TryArgs {
                    entity: entity.clone(),
                    kind: *kind,
                    claim: claim.clone(),
                    grounds: grounds.clone(),
                    desc: desc.clone(),
                    depends_on: depends_on.clone(),
                    cog_dir,
                },
            ),
            Start {
                entity,
                description,
                max_nodes,
            } => command::experiment_cmd::start(
                store,
                entity,
                description.clone(),
                *max_nodes,
                cog_dir,
            ),
            Hypothesize {
                id,
                entity,
                kind,
                claim,
                grounds,
            } => command::experiment_cmd::hypothesize(id, entity, *kind, claim, grounds, cog_dir),
            HypotheticalDelete { id, entity } => {
                command::experiment_cmd::hypothesize_delete(id, entity, cog_dir)
            }
            HypotheticalRelation { id, from, to, kind } => {
                command::experiment_cmd::hypothesize_relation(id, from, to, *kind, cog_dir)
            }
            Evaluate { id } => command::experiment_cmd::evaluate(id, cog_dir),
            Report { id } => command::experiment_cmd::report(id, cog_dir),
            Commit { id } => command::experiment_cmd::commit(store, id, cog_dir),
            Discard { id } => command::experiment_cmd::discard(id, cog_dir),
            List => command::experiment_cmd::list(cog_dir),
            Save { id } => command::experiment_cmd::save(id, cog_dir),
            Load { id } => command::experiment_cmd::load(id, cog_dir),
        }
    }

    fn run_backup(&self, action: &BackupAction, store: &SqliteRepository) -> Result<CommandOutput> {
        let mgr = BackupManager::new(&self.db_path());
        use BackupAction::*;
        match action {
            Create { name } => command::backup_cmd::create(store, &mgr, name.clone()),
            List => command::backup_cmd::list(&mgr),
            Restore { name } => command::backup_cmd::restore(&mgr, name),
            Drop { name } => command::backup_cmd::drop(&mgr, name),
        }
    }

    fn apply_workflow_then_save(
        &self,
        wf: &mut WorkflowState,
        cog_dir: &Path,
        result: Result<CommandOutput>,
    ) -> Result<CommandOutput> {
        let _ = wf.save(cog_dir);
        result
    }
}
