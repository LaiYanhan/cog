use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::command::{self, CommandOutput};
use crate::domain::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind, ExportFormat};
use crate::backup::BackupManager;
use crate::repo::BranchManager;
use crate::repo::SqliteRepository;
use crate::workflow::{WorkflowState, suggest_actions};

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
    /// [DEPRECATED: use 'cog experiment' or 'cog backup' instead] Manage model branches
    Branch {
        #[command(subcommand)]
        action: BranchAction,
    },
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
    /// Show suggested next actions based on current workflow state
    Next,
    /// Begin tracking a code change
    StartChange {
        /// Description of the change
        description: String,
        /// Entities expected to be affected
        #[arg(long)]
        entity: Vec<String>,
    },
    /// Finish the current change cycle
    FinishChange,
    /// Abort the current change cycle
    AbortChange,
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

#[derive(Debug, Subcommand)]
pub enum ExperimentAction {
    /// Start a new experiment focused on an entity
    Start {
        /// Entity to focus the experiment on
        entity: String,
        /// Description of the experiment
        #[arg(long)]
        description: Option<String>,
        /// Maximum nodes to load into the experiment subgraph
        #[arg(long, default_value = "500")]
        max_nodes: usize,
    },
    /// Add a hypothetical assertion to the experiment
    Hypothesize {
        /// Experiment ID (short or full)
        id: String,
        /// Entity to assert about
        #[arg(long)]
        entity: String,
        /// Kind of assertion
        #[arg(long)]
        kind: AssertionKind,
        /// The claim
        #[arg(long)]
        claim: String,
        /// Grounds for the claim
        #[arg(long)]
        grounds: String,
    },
    /// Evaluate the experiment — simulate cascade and detect contradictions
    Evaluate {
        /// Experiment ID
        id: String,
    },
    /// Show the experiment report
    Report {
        /// Experiment ID
        id: String,
    },
    /// Commit the experiment to the real model
    Commit {
        /// Experiment ID
        id: String,
    },
    /// Discard the experiment without changes
    Discard {
        /// Experiment ID
        id: String,
    },
    /// List all saved experiments
    List,
    /// Save the current experiment state to disk
    Save {
        /// Experiment ID
        id: String,
    },
    /// Load a saved experiment from disk
    Load {
        /// Experiment ID
        id: String,
    },
}

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
            Commands::Query { entity, all } => {
                let out = command::query::execute(store, entity, *all)?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Impact { entity } => {
                let out = command::impact::execute(store, entity)?;
                wf.transition_assess();
                Ok(out)
            }
            Commands::Trace { entity } => {
                let out = command::trace::execute(store, entity)?;
                wf.transition_assess();
                Ok(out)
            }
            Commands::Index {
                kind,
                origin,
                prefix,
            } => {
                let out = command::index_cmd::execute(store, *kind, *origin, prefix.as_deref())?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Assert {
                entity,
                kind,
                claim,
                grounds,
                depends_on,
            } => {
                let out = command::assert_cmd::execute(
                    store,
                    entity,
                    *kind,
                    claim,
                    grounds,
                    depends_on.as_deref(),
                )?;
                wf.transition_explore();
                Ok(out)
            }
            Commands::Retract { id, reason } => {
                let out = command::retract::execute(store, id, reason)?;
                wf.transition_retract();
                Ok(out)
            }
            Commands::Depend { entity_a, on, kind } => {
                let out = command::depend::execute(store, entity_a, on, *kind)?;
                wf.transition_explore();
                Ok(out)
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
                let out = command::verify::execute(store, scope.as_deref(), *clean, resolved)?;
                let passed = out.exit_code == 0;
                wf.transition_verify(passed);
                Ok(out)
            }
            Commands::Export { format } => {
                let out = command::export::execute(store, *format)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::Stats => {
                let out = command::stats::execute(store)?;
                wf.transition_browse();
                Ok(out)
            }
            Commands::DeleteEntity { entity } => {
                let out = command::entity_cmd::execute(store, entity)?;
                // delete-entity doesn't change state but browse is safe
                Ok(out)
            }
            Commands::Branch { action } => {
                let mgr = BranchManager::new(&self.db_path());
                command::branch_cmd::execute(store, &mgr, action)
            }
            Commands::Experiment { action } => {
                use ExperimentAction::*;
                match action {
                    Start { entity, description, max_nodes } => {
                        command::experiment_cmd::start(store, entity, description.clone(), *max_nodes, &cog_dir)
                    }
                    Hypothesize { id, entity, kind, claim, grounds } => {
                        command::experiment_cmd::hypothesize(id, entity, *kind, claim, grounds, &cog_dir)
                    }
                    Evaluate { id } => {
                        command::experiment_cmd::evaluate(id, &cog_dir)
                    }
                    Report { id } => {
                        command::experiment_cmd::report(id, &cog_dir)
                    }
                    Commit { id } => {
                        command::experiment_cmd::commit(store, id, &cog_dir)
                    }
                    Discard { id } => {
                        command::experiment_cmd::discard(id, &cog_dir)
                    }
                    List => {
                        command::experiment_cmd::list(&cog_dir)
                    }
                    Save { id } => {
                        command::experiment_cmd::save(id, &cog_dir)
                    }
                    Load { id } => {
                        command::experiment_cmd::load(id, &cog_dir)
                    }
                }
            }
            Commands::Backup { action } => {
                let mgr = BackupManager::new(&self.db_path());
                use BackupAction::*;
                match action {
                    Create { name } => {
                        command::backup_cmd::create(&mgr, name.clone())
                    }
                    List => {
                        command::backup_cmd::list(&mgr)
                    }
                    Restore { name } => {
                        command::backup_cmd::restore(&mgr, name)
                    }
                    Drop { name } => {
                        command::backup_cmd::drop(&mgr, name)
                    }
                }
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
                let out =
                    command::init_cmd::execute(store, &scan_path, *dry_run, *depth, lang_list)?;
                if out.exit_code == 0 {
                    wf.transition_init().ok(); // may fail if already init'd
                }
                Ok(out)
            }
            Commands::Next => {
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
            Commands::StartChange {
                description,
                entity,
            } => {
                wf.transition_start_change(description.clone(), entity.clone())?;
                let out = CommandOutput::success(format!(
                    "started change: {}\nState: {}",
                    description,
                    wf.describe()
                ));
                Ok(out)
            }
            Commands::FinishChange => {
                wf.transition_finish_change()?;
                Ok(CommandOutput::success(format!(
                    "change finished.\nState: {}",
                    wf.describe()
                )))
            }
            Commands::AbortChange => {
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
        crate::workflow::ActionKind::RecordMissingContracts { .. } => "record_contracts",
        crate::workflow::ActionKind::ReviewUncertainAssertions { .. } => "review_uncertain",
        crate::workflow::ActionKind::StartRecording => "start_recording",
        crate::workflow::ActionKind::AssessImpact { .. } => "assess_impact",
        crate::workflow::ActionKind::StartChange => "start_change",
        crate::workflow::ActionKind::VerifyChanges => "verify",
        crate::workflow::ActionKind::RecordFix { .. } => "record_fix",
        crate::workflow::ActionKind::FinishChange => "finish_change",
        crate::workflow::ActionKind::AbortChange => "abort_change",
        crate::workflow::ActionKind::TraceRootCause => "trace",
        crate::workflow::ActionKind::VerifyConsistency => "verify",
        crate::workflow::ActionKind::StartExperiment => "experiment",
        crate::workflow::ActionKind::StartExperimentDuringChange => "experiment",
    }
}
