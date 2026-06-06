mod args;
mod backup;
mod experiment;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

pub use args::*;
pub use backup::BackupAction;
pub use experiment::ExperimentAction;

use crate::backup::BackupManager;
use crate::command::{self, CommandOutput};
use crate::repo::SqliteRepository;
use crate::workflow::{WorkflowState, suggest_actions};

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
    /// Scan a codebase and populate the model with structural entities
    Init(InitArgs),
    /// Show suggested next actions based on current workflow state
    Next(NextArgs),
    /// Begin tracking a code change
    StartChange(StartChangeArgs),
    /// Finish the current change cycle
    FinishChange(FinishChangeArgs),
    /// Abort the current change cycle
    AbortChange(AbortChangeArgs),
}

impl Cli {
    pub fn db_path(&self) -> PathBuf {
        self.db
            .clone()
            .unwrap_or_else(|| PathBuf::from(".cog/cog.db"))
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
                let out = command::query::execute(store, &args.entity, args.all, self.output)?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Impact(args) => {
                let out = command::impact::execute(store, &args.entity, self.output)?;
                wf.transition_assess();
                Ok(out)
            }
            Commands::Trace(args) => {
                let out = command::trace::execute(store, &args.entity, self.output)?;
                wf.transition_assess();
                Ok(out)
            }
            Commands::Index(args) => {
                let out = command::index_cmd::execute(
                    store,
                    args.kind,
                    args.origin,
                    args.prefix.as_deref(),
                    self.output,
                )?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Assert(args) => {
                let out = command::assert_cmd::execute(
                    store,
                    &args.entity,
                    args.kind,
                    &args.claim,
                    &args.grounds,
                    args.depends_on.as_deref(),
                    self.output,
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
            Commands::Experiment { action } => {
                use ExperimentAction::*;
                match action {
                    Start {
                        entity,
                        description,
                        max_nodes,
                    } => command::experiment_cmd::start(
                        store,
                        entity,
                        description.clone(),
                        *max_nodes,
                        &cog_dir,
                    ),
                    Hypothesize {
                        id,
                        entity,
                        kind,
                        claim,
                        grounds,
                    } => command::experiment_cmd::hypothesize(
                        id, entity, *kind, claim, grounds, &cog_dir,
                    ),
                    HypotheticalDelete { id, entity } => {
                        command::experiment_cmd::hypothesize_delete(id, entity, &cog_dir)
                    }
                    HypotheticalRelation { id, from, to, kind } => {
                        command::experiment_cmd::hypothesize_relation(id, from, to, *kind, &cog_dir)
                    }
                    Evaluate { id } => command::experiment_cmd::evaluate(id, &cog_dir),
                    Report { id } => command::experiment_cmd::report(id, &cog_dir),
                    Commit { id } => command::experiment_cmd::commit(store, id, &cog_dir),
                    Discard { id } => command::experiment_cmd::discard(id, &cog_dir),
                    List => command::experiment_cmd::list(&cog_dir),
                    Save { id } => command::experiment_cmd::save(id, &cog_dir),
                    Load { id } => command::experiment_cmd::load(id, &cog_dir),
                }
            }
            Commands::Backup { action } => {
                let mgr = BackupManager::new(&self.db_path());
                use BackupAction::*;
                match action {
                    Create { name } => command::backup_cmd::create(store, &mgr, name.clone()),
                    List => command::backup_cmd::list(&mgr),
                    Restore { name } => command::backup_cmd::restore(&mgr, name),
                    Drop { name } => command::backup_cmd::drop(&mgr, name),
                }
            }
            Commands::Init(args) => {
                let scan_path = args
                    .path
                    .as_deref()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                let lang_list: Option<Vec<String>> = args
                    .lang
                    .as_ref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
                let out = command::init_cmd::execute(
                    store,
                    &scan_path,
                    args.dry_run,
                    args.depth,
                    lang_list,
                    self.output,
                )?;
                if out.exit_code == 0 {
                    wf.transition_init().ok(); // may fail if already init'd
                }
                Ok(out)
            }
            Commands::Next(_) => {
                let actions = suggest_actions(&wf, store);
                let mut text = format!("State: {}\n\nSuggested actions:\n", wf.describe());
                for (i, a) in actions.iter().enumerate() {
                    text.push_str(&format!(
                        "  {}. [{}] {}\n     Why: {}\n     Example: {}\n",
                        i + 1,
                        action_kind_label(&a.action),
                        a.description,
                        a.why,
                        a.example_command,
                    ));
                }
                Ok(CommandOutput::success(text))
            }
            Commands::StartChange(args) => {
                wf.transition_start_change(args.description.clone(), Vec::new())?;
                let out = CommandOutput::success(format!(
                    "started change: {}\nState: {}",
                    args.description,
                    wf.describe()
                ));
                Ok(out)
            }
            Commands::FinishChange(_) => {
                wf.transition_finish_change()?;
                Ok(CommandOutput::success(format!(
                    "change finished.\nState: {}",
                    wf.describe()
                )))
            }
            Commands::AbortChange(_) => {
                wf.transition_abort_change()?;
                Ok(CommandOutput::success(format!(
                    "change aborted.\nState: {}",
                    wf.describe()
                )))
            }
        };

        // Persist workflow state after every command
        let _ = wf.save(&cog_dir);
        result
    }
}

fn action_kind_label(kind: &crate::workflow::ActionKind) -> &'static str {
    match kind {
        crate::workflow::ActionKind::InitProject => "init",
        crate::workflow::ActionKind::RecordMissingContracts => "record_contracts",
        crate::workflow::ActionKind::ReviewUncertainAssertions => "review_uncertain",
        crate::workflow::ActionKind::StartRecording => "start_recording",
        crate::workflow::ActionKind::AssessImpact => "assess_impact",
        crate::workflow::ActionKind::StartChange => "start_change",
        crate::workflow::ActionKind::VerifyChanges => "verify",
        crate::workflow::ActionKind::RecordFix => "record_fix",
        crate::workflow::ActionKind::FinishChange => "finish_change",
        crate::workflow::ActionKind::AbortChange => "abort_change",
        crate::workflow::ActionKind::TraceRootCause => "trace",
        crate::workflow::ActionKind::VerifyConsistency => "verify",
        crate::workflow::ActionKind::StartExperiment => "experiment",
        crate::workflow::ActionKind::StartExperimentDuringChange => "experiment",
    }
}
