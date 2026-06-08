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
use crate::format;
use crate::repo::{Repository, SqliteRepository};
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
    /// Show suggested next actions based on current workflow state
    Next(NextArgs),
    /// Scan a codebase and sync the model (idempotent — safe to re-run)
    Sync(SyncArgs),
}

impl Cli {
    pub fn db_path(&self) -> PathBuf {
        if let Some(ref p) = self.db {
            return p.clone();
        }
        // Walk up from CWD to find .cog/cog.db, like git finds .git
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join(".cog").join("cog.db");
            if candidate.exists() {
                return candidate;
            }
            match dir.parent() {
                Some(parent) => dir = parent,
                None => break,
            }
        }
        // No existing .cog/ found — default to CWD-relative for `cog init`
        PathBuf::from(".cog/cog.db")
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
                            cog_dir: &cog_dir,
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
            Commands::Sync(args) => {
                let scan_path = args
                    .path
                    .as_deref()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                let lang_list: Option<Vec<String>> = args
                    .lang
                    .as_ref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
                let out = command::sync_cmd::execute(
                    store,
                    &scan_path,
                    args.dry_run,
                    args.depth,
                    lang_list,
                    self.output,
                )?;
                if out.exit_code == 0 && !args.dry_run {
                    if matches!(wf, WorkflowState::Uninit) {
                        wf.transition_init().ok();
                    } else {
                        wf.transition_sync(out.has_drift);
                    }
                }
                Ok(out)
            }
            Commands::Next(_) => {
                let active_experiments = detect_active_experiments(&cog_dir);
                let actions = suggest_actions(&wf, store, &active_experiments);
                let stats = store.stats().unwrap_or_default();

                // Separate stagnation SyncModel from regular suggestions
                let mut suggestions = Vec::new();
                let mut stagnation_warning = None;
                for a in &actions {
                    if matches!(a.action, crate::workflow::ActionKind::SyncModel) {
                        stagnation_warning = Some(format!(
                            "WARNING: {}\n  Next: {}",
                            a.description, a.example_command
                        ));
                    } else {
                        suggestions.push(crate::domain::NextSuggestion {
                            kind: action_kind_label(&a.action).to_string(),
                            description: a.description.clone(),
                            next_command: a.example_command.clone(),
                        });
                    }
                }

                let coverage_pct = if stats.entities > 0 {
                    (stats.covered_entities as f64) / (stats.entities as f64) * 100.0
                } else {
                    0.0
                };

                let report = crate::domain::NextReport {
                    state: wf.describe(),
                    active_experiments,
                    model: crate::domain::NextModelSummary {
                        entities: stats.entities,
                        assertions: stats.assertions,
                        active: stats.active_assertions,
                        retracted: stats.retracted_assertions,
                    },
                    covered: stats.covered_entities,
                    coverage_pct,
                    suggestions,
                    stagnation_warning,
                };

                Ok(CommandOutput::success(format::emit_report(
                    &report,
                    self.output,
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
        crate::workflow::ActionKind::RecordMissingContracts => "model",
        crate::workflow::ActionKind::ReviewUncertainAssertions => "review",
        crate::workflow::ActionKind::StartRecording => "model",
        crate::workflow::ActionKind::AssessImpact => "assess",
        crate::workflow::ActionKind::RecordFix => "model",
        crate::workflow::ActionKind::TraceRootCause => "trace",
        crate::workflow::ActionKind::VerifyConsistency => "verify",
        crate::workflow::ActionKind::StartExperiment => "descent",
        crate::workflow::ActionKind::SyncModel => "drift",
        crate::workflow::ActionKind::ImplementNow => "descent",
        crate::workflow::ActionKind::CommitExperiment => "descent",
    }
}

/// Detect all active (Open/Evaluated) experiments from disk.
/// Returns a list of `ActiveExperiment` sorted by modification time (most recent first).
fn detect_active_experiments(cog_dir: &std::path::Path) -> Vec<crate::domain::ActiveExperiment> {
    let exp_dir = cog_dir.join("experiments");
    if !exp_dir.exists() {
        return Vec::new();
    }
    let entries = match std::fs::read_dir(&exp_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results: Vec<crate::domain::ActiveExperiment> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };

        // Quick parse: look for "status" field
        let Some(status_pos) = content.find("\"status\":") else {
            continue;
        };
        let rest = &content[status_pos + 9..];
        let trimmed = rest.trim_start();

        let status_label = if trimmed.starts_with("\"Evaluated\"") {
            "evaluated"
        } else if trimmed.starts_with("\"Open\"") {
            "draft"
        } else {
            continue; // Committed/Discarded — not active
        };

        let short_id = content
            .find("\"id\":")
            .and_then(|i| {
                let r = &content[i + 5..];
                let start = r.find('"')? + 1;
                let end = r[start..].find('"')?;
                Some(&r[start..start + end])
            })
            .map(|id| if id.len() >= 8 { &id[..8] } else { id })
            .unwrap_or("unknown")
            .to_string();

        let description = content
            .find("\"description\":")
            .and_then(|i| {
                let r = &content[i + 15..];
                let start = r.find('"')? + 1;
                let end = r[start..].find('"')?;
                Some(&r[start..start + end])
            })
            .unwrap_or("")
            .to_string();

        // Use file mtime as proxy for evaluation time
        let mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| {
                chrono::DateTime::from_timestamp(
                    t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                    0,
                )
            });

        results.push(crate::domain::ActiveExperiment {
            short_id,
            description,
            status: status_label.to_string(),
            mtime,
        });
    }

    // Sort by mtime descending (most recent first)
    results.sort_by_key(|b| std::cmp::Reverse(b.mtime));
    results
}
