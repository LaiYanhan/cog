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
use crate::workflow::state::WorkflowPhase;
#[derive(Debug, Parser)]
#[command(name = "cog", about = "Cognitive model for coding agents", version)]
pub struct Cli {
    /// Path to the cognitive model database
    #[arg(long, env = "COG_DB")]
    pub db: Option<PathBuf>,

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
    /// Move all assertions and relations from one entity onto another (reconcile design/code names)
    Migrate(MigrateArgs),
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
    /// Recover Uncertain assertions whose dependencies are all Active again
    Recover {
        /// Apply recovery (default is dry-run preview)
        #[arg(long)]
        apply: bool,
    },
    /// Show suggested next actions based on current workflow state
    Next(NextArgs),
    /// Sync the cognitive model with the codebase (idempotent). Use --init to create a new model.
    Sync(SyncArgs),

    /// Show local usage statistics (command frequency, read/write ratio, sessions)
    Usage(UsageArgs),
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

    pub fn run(&self, store: &SqliteRepository) -> Result<CommandOutput> {
        let cog_dir = self
            .db_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        // Ensure .cog/ dir exists for workflow state
        let _ = std::fs::create_dir_all(&cog_dir);
        let mut wf = WorkflowState::load(&cog_dir);
        let phase_before = self.phase_label(&wf);

        let started = std::time::Instant::now();
        let result = self.dispatch(&mut wf, store, &cog_dir);
        let duration_ms = started.elapsed().as_millis() as u64;

        // Record usage (best-effort; never breaks the command). Skip the
        // `usage` command itself so reading the log doesn't pollute it.
        if !matches!(self.command, Commands::Usage(_)) {
            let phase_after = self.phase_label(&wf);
            let (phase_from, phase_to) = if phase_before != phase_after {
                (Some(phase_before.clone()), Some(phase_after))
            } else {
                (None, None)
            };
            crate::usage::recorder::record(
                &cog_dir,
                &crate::usage::UsageEvent {
                    ts: chrono::Utc::now(),
                    command: self.command_name(),
                    ok: result.is_ok(),
                    exit_code: result.as_ref().ok().map(|o| o.exit_code),
                    duration_ms,
                    has_drift: result.as_ref().ok().map(|o| o.has_drift).unwrap_or(false),
                    phase_from,
                    phase_to,
                    args: self.command_args(),
                    metrics: result.as_ref().ok().and_then(|o| o.metrics.clone()),
                },
            );
        }

        self.apply_workflow_then_save(&mut wf, &cog_dir, result)
    }

    /// The actual command dispatch. Every command flows through here, so this
    /// is the single chokepoint for workflow transitions. Extracted from `run`
    /// so usage can be recorded on both the Ok and Err paths.
    fn dispatch(
        &self,
        wf: &mut WorkflowState,
        store: &SqliteRepository,
        cog_dir: &Path,
    ) -> Result<CommandOutput> {
        match &self.command {
            Commands::Query(args) => {
                let out = command::query::execute(
                    store,
                    &args.entity,
                    args.all,
                    args.compact,
                    args.relations,
                    self.output,
                )?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Impact(args) => {
                let out = command::impact::execute(store, &args.entity, self.output)?;
                Ok(out)
            }
            Commands::Trace(args) => {
                let out = command::trace::execute(store, &args.entity, self.output)?;
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
                        replace_id: args.replace.as_deref(),
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
                Ok(out)
            }
            Commands::Stats(_) => {
                let out = command::stats::execute(store, self.output)?;
                Ok(out)
            }
            Commands::DeleteEntity(args) => {
                let out = command::entity_cmd::execute(store, &args.entity)?;
                // delete-entity doesn't change phase (design §5.1) — next `cog next`
                // may suggest verify due to the destructive operation.
                Ok(out)
            }
            Commands::Migrate(args) => {
                let out = command::migrate_cmd::execute(store, &args.from, &args.to)?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Experiment { action } => {
                let out = self.run_experiment(action, cog_dir, store)?;
                if matches!(action, ExperimentAction::Commit { .. }) {
                    // Model updated but code not yet changed — track the gap
                    if let WorkflowState::Ready { phase } = &mut *wf {
                        *phase = WorkflowPhase::PendingImplement;
                    }
                }
                Ok(out)
            }
            Commands::Backup { action } => self.run_backup(action, store),
            Commands::Sync(args) => {
                let lang_list: Option<Vec<String>> = args
                    .lang
                    .as_ref()
                    .map(|s| s.split(',').map(|l| l.trim().to_string()).collect());
                let db = self.db_path();
                let dry_run = args.dry_run;
                let output = self.output;
                // No outer transaction: sync is idempotent and re-runnable, and
                // delete_entity (stale cleanup) manages its own transaction —
                // wrapping here nests BEGIN IMMEDIATE and crashes on stale removal.
                let out = command::sync_cmd::execute(store, &db, dry_run, lang_list, output)?;
                if out.exit_code == 0 && !dry_run {
                    if matches!(wf, WorkflowState::Uninit) {
                        wf.transition_init().ok();
                    } else {
                        wf.transition_sync(out.has_drift);
                    }
                }
                Ok(out)
            }
            Commands::Recover { apply } => {
                let out = command::recover::execute(store, *apply, self.output)?;
                if *apply {
                    wf.transition_explore();
                }
                Ok(out)
            }
            Commands::Next(_) => command::next_cmd::execute(store, wf, cog_dir, self.output),
            Commands::Usage(args) => {
                command::usage_cmd::execute(store, cog_dir, args.raw, self.output)
            }
        }
    }

    /// Command verb for the usage log.
    fn command_name(&self) -> String {
        match &self.command {
            Commands::Query(_) => "query",
            Commands::Impact(_) => "impact",
            Commands::Trace(_) => "trace",
            Commands::Index(_) => "index",
            Commands::Assert(_) => "assert",
            Commands::Retract(_) => "retract",
            Commands::Depend(_) => "depend",
            Commands::Verify(_) => "verify",
            Commands::Export(_) => "export",
            Commands::Stats(_) => "stats",
            Commands::DeleteEntity(_) => "delete-entity",
            Commands::Migrate(_) => "migrate",
            Commands::Experiment { .. } => "experiment",
            Commands::Backup { .. } => "backup",
            Commands::Recover { .. } => "recover",
            Commands::Next(_) => "next",
            Commands::Sync(_) => "sync",
            Commands::Usage(_) => "usage",
        }
        .to_string()
    }

    /// Structured args for the usage log — entity refs, IDs, kinds, flags only.
    /// Never free-text prose (claims/reasons stay in cog.db).
    fn command_args(&self) -> serde_json::Value {
        match &self.command {
            Commands::Query(a) => serde_json::json!({
                "entity": a.entity,
                "all": a.all,
                "compact": a.compact,
                "relations": a.relations
            }),
            Commands::Impact(a) => serde_json::json!({ "entity": a.entity }),
            Commands::Trace(a) => serde_json::json!({ "entity": a.entity }),
            Commands::Index(a) => serde_json::json!({
                "kind": format!("{:?}", a.kind),
                "origin": format!("{:?}", a.origin),
                "prefix": a.prefix,
                "verbose": a.verbose,
                "uncovered": a.uncovered
            }),
            Commands::Assert(a) => serde_json::json!({
                "entity": a.entity,
                "kind": format!("{:?}", a.kind),
                "grounds": a.grounds,
                "depends_on": a.depends_on,
                "replace": a.replace,
                "force": a.force
            }),
            Commands::Retract(a) => serde_json::json!({ "id": a.id }),
            Commands::Depend(a) => serde_json::json!({
                "entity_a": a.entity_a,
                "on": a.on,
                "kind": format!("{:?}", a.kind)
            }),
            Commands::Verify(a) => serde_json::json!({
                "scope": a.scope,
                "clean": a.clean,
                "scan": a.scan
            }),
            Commands::Export(a) => serde_json::json!({ "format": format!("{:?}", a.format) }),
            Commands::Stats(_) => serde_json::json!({}),
            Commands::DeleteEntity(a) => serde_json::json!({ "entity": a.entity }),
            Commands::Migrate(a) => serde_json::json!({ "from": a.from, "to": a.to }),
            Commands::Sync(a) => serde_json::json!({
                "init": a.init,
                "dry_run": a.dry_run,
                "lang": a.lang
            }),
            Commands::Next(_) => serde_json::json!({}),
            Commands::Usage(a) => serde_json::json!({ "raw": a.raw }),
            Commands::Experiment { action } => {
                serde_json::json!({ "sub": self.variant_name(action) })
            }
            Commands::Backup { action } => {
                serde_json::json!({ "sub": self.variant_name(action) })
            }
            Commands::Recover { apply } => serde_json::json!({ "apply": apply }),
        }
    }

    /// Debug-print an enum value, then take the variant name before any
    /// `(` or `{` — e.g. `Commit { id: ".." }` → `"commit"`, `List` → `"list"`.
    fn variant_name<T: std::fmt::Debug>(&self, v: &T) -> String {
        let s = format!("{v:?}");
        s.split(['(', '{'])
            .next()
            .unwrap_or(&s)
            .trim()
            .to_lowercase()
    }

    /// Workflow phase as a stable string label, for the usage log.
    fn phase_label(&self, wf: &WorkflowState) -> String {
        match wf {
            WorkflowState::Uninit => "uninit".to_string(),
            WorkflowState::Ready { phase } => match phase {
                WorkflowPhase::FreshScan => "fresh_scan",
                WorkflowPhase::Exploring => "exploring",
                WorkflowPhase::PendingImplement => "pending_implement",
                WorkflowPhase::PostChange => "post_change",
                WorkflowPhase::Debugging => "debugging",
            }
            .to_string(),
        }
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
            Restore { name } => {
                store.checkpoint_wal()?;
                command::backup_cmd::restore(&mgr, name)
            }
            Drop { name } => command::backup_cmd::drop(&mgr, name),
        }
    }

    fn apply_workflow_then_save(
        &self,
        wf: &mut WorkflowState,
        cog_dir: &Path,
        result: Result<CommandOutput>,
    ) -> Result<CommandOutput> {
        if let Err(e) = wf.save(cog_dir) {
            // Log but don't fail — the command output is more important.
            eprintln!("warning: failed to save workflow state: {e}");
        }
        result
    }
}
