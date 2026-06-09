use crate::domain::{ChangelogAction, ChangelogEntry, ModelStats};
use crate::repo::Repository;
use crate::workflow::state::{WorkflowPhase, WorkflowState};

// ── Thresholds ──────────────────────────────────────────────────────────────

const STAGNATION_WINDOW: usize = 5;
const COVERAGE_IMPLEMENT_THRESHOLD: f64 = 60.0;
const COVERAGE_REFINE_THRESHOLD: f64 = 80.0;

// ── Suggested action types ──────────────────────────────────────────────────

/// A suggestion the CLI offers to the agent — "what you can do next".
#[derive(Debug, Clone)]
pub struct SuggestedAction {
    pub action: ActionKind,
    pub description: String,
    pub example_command: String,
}

#[derive(Debug, Clone)]
pub enum ActionKind {
    InitProject,
    RecordMissingContracts,
    ReviewUncertainAssertions,
    StartRecording,
    AssessImpact,
    RecordFix,
    TraceRootCause,
    VerifyConsistency,
    StartExperiment,
    SyncModel,
    ImplementNow,
    CommitExperiment,
}

// ── Public entry point ──────────────────────────────────────────────────────

/// Returns suggested actions based on current workflow state, model data, and
/// active experiments.  See CLI_V2 §7.3 for the full decision table.
pub fn suggest_actions(
    state: &WorkflowState,
    repo: &dyn Repository,
    active_experiments: &[crate::domain::ActiveExperiment],
) -> Vec<SuggestedAction> {
    match state {
        WorkflowState::Uninit => vec![SuggestedAction {
            action: ActionKind::InitProject,
            description: "No cognitive model found. Run sync to scan the codebase.".into(),
            example_command: "cog sync".into(),
        }],
        WorkflowState::Ready { phase } => suggest_for_ready(phase, repo, active_experiments),
    }
}

// ── Phase × ActiveExperiments decision table ────────────────────────────────

/// Derive a simple experiment summary: count of drafts and evaluated.
fn experiment_summary(experiments: &[crate::domain::ActiveExperiment]) -> (usize, usize) {
    let drafts = experiments.iter().filter(|e| e.status == "draft").count();
    let evaluated = experiments
        .iter()
        .filter(|e| e.status == "evaluated")
        .count();
    (drafts, evaluated)
}

fn suggest_for_ready(
    phase: &WorkflowPhase,
    repo: &dyn Repository,
    active_experiments: &[crate::domain::ActiveExperiment],
) -> Vec<SuggestedAction> {
    let stats = repo.stats().unwrap_or_default();
    let coverage_pct = compute_coverage_pct(&stats);
    let changelog = repo.list_changelog_entries().unwrap_or_default();
    let (drafts, evaluated) = experiment_summary(active_experiments);

    let mut actions = Vec::new();

    // ── Phase-independent experiment priority ────────────────────────────
    // Draft experiments always take priority: the agent started an experiment
    // and should finish evaluating it before starting new work.
    if drafts > 0 {
        actions.push(SuggestedAction {
            action: ActionKind::StartExperiment,
            description: format!(
                "{} draft experiment(s) pending evaluation. Finish before starting new work.",
                drafts
            ),
            example_command: "cog experiment evaluate <id>".into(),
        });
    }

    // Evaluated experiments: suggest commit or discard (but don't block other suggestions)
    if evaluated > 0 {
        actions.push(SuggestedAction {
            action: ActionKind::CommitExperiment,
            description: format!(
                "{} evaluated experiment(s) ready. Commit to proceed or discard.",
                evaluated
            ),
            example_command: "cog experiment commit <id>".into(),
        });
    }

    // ── Phase-specific suggestions ───────────────────────────────────────
    match phase {
        WorkflowPhase::FreshScan => {
            actions.push(SuggestedAction {
                action: ActionKind::StartRecording,
                description: format!(
                    "{} entities found. Start recording assertions.",
                    stats.entities
                ),
                example_command: "cog query <core_entity>".into(),
            });
            let orphans = count_orphan_entities(repo, &stats);
            if orphans > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::RecordMissingContracts,
                    description: format!("{orphans} entities have no assertions yet."),
                    example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
                });
            }
        }

        WorkflowPhase::Exploring => {
            actions.push(SuggestedAction {
                action: ActionKind::AssessImpact,
                description: "Run impact analysis to understand downstream dependencies.".into(),
                example_command: "cog impact <core_entity>".into(),
            });

            if coverage_pct > COVERAGE_REFINE_THRESHOLD {
                actions.push(SuggestedAction {
                    action: ActionKind::VerifyConsistency,
                    description: format!(
                        "Coverage is {:.0}%. Verify consistency and refine existing assertions.",
                        coverage_pct
                    ),
                    example_command: "cog verify".into(),
                });
                actions.push(SuggestedAction {
                    action: ActionKind::ImplementNow,
                    description: "Good coverage. Consider starting implementation.".into(),
                    example_command: "cog experiment try <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\"".into(),
                });
            } else if coverage_pct > COVERAGE_IMPLEMENT_THRESHOLD {
                actions.push(SuggestedAction {
                    action: ActionKind::ImplementNow,
                    description: format!(
                        "Coverage is {:.0}%. Consider a sandbox experiment before implementing.",
                        coverage_pct
                    ),
                    example_command: "cog experiment try <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\"".into(),
                });
            } else {
                let orphans = count_orphan_entities(repo, &stats);
                if orphans > 0 {
                    actions.push(SuggestedAction {
                        action: ActionKind::RecordMissingContracts,
                        description: format!("{orphans} entities have no assertions yet."),
                        example_command: "cog assert <entity> --kind contract --claim \"...\""
                            .into(),
                    });
                }
            }
        }

        WorkflowPhase::PostChange => {
            actions.push(SuggestedAction {
                action: ActionKind::RecordFix,
                description: "Code changed. Record corrections for changed entities.".into(),
                example_command: "cog assert <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\"".into(),
            });
        }

        WorkflowPhase::Debugging => {
            if stats.uncertain_assertions > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::ReviewUncertainAssertions,
                    description: format!(
                        "{} assertions are uncertain since last retraction.",
                        stats.uncertain_assertions
                    ),
                    example_command: "cog query <affected_entity>".into(),
                });
            }
            actions.push(SuggestedAction {
                action: ActionKind::TraceRootCause,
                description: "Trace dependency chains to find root causes.".into(),
                example_command: "cog trace <entity>".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::VerifyConsistency,
                description: "Run verify to check structural consistency.".into(),
                example_command: "cog verify".into(),
            });
        }
    }

    // ── Stagnation detection ────────────────────────────────────────────
    if let Some(warning) = detect_stagnation(&changelog, active_experiments) {
        actions.push(warning);
    }

    actions
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Compute coverage percentage from model stats.
fn compute_coverage_pct(stats: &ModelStats) -> f64 {
    if stats.entities == 0 {
        return 100.0;
    }
    (stats.covered_entities as f64) / (stats.entities as f64) * 100.0
}

/// Count entities with zero assertions.
fn count_orphan_entities(repo: &dyn Repository, stats: &ModelStats) -> usize {
    if stats.active_assertions == 0 && stats.retracted_assertions == 0 {
        // Fast path: no assertions at all → all entities are orphans
        stats.entities as usize
    } else {
        repo.count_unasserted_entities().unwrap_or(0) as usize
    }
}

/// Detect stagnation based on changelog patterns and stale experiments.
///
/// Three rules:
/// 1. **Verify loop**: recent N changelog entries are all Verify → agent stuck
///    in a verify-only loop without making progress.
/// 2. **Stale evaluated experiment**: an Evaluated experiment has been sitting
///    on disk with >= STAGNATION_WINDOW changelog entries written after its
///    file modification time — the agent forgot to commit or discard it.
/// 3. **Guard**: dense assert activity (all recent = Assert) never triggers.
fn detect_stagnation(
    changelog: &[ChangelogEntry],
    active_experiments: &[crate::domain::ActiveExperiment],
) -> Option<SuggestedAction> {
    if changelog.is_empty() {
        return None;
    }

    // Sort by timestamp descending and take the last STAGNATION_WINDOW.
    let mut recent: Vec<&ChangelogEntry> = changelog.iter().collect();
    recent.sort_by_key(|e| &e.timestamp);
    let window: Vec<&ChangelogEntry> = recent
        .iter()
        .rev()
        .take(STAGNATION_WINDOW)
        .copied()
        .collect();

    if window.is_empty() {
        return None;
    }

    // Guard: active modeling — all recent actions are Assert.  Never fire.
    if window
        .iter()
        .all(|e| matches!(e.action, ChangelogAction::Assert))
    {
        return None;
    }

    // Rule 1: verify-only loop.
    let all_verify = window
        .iter()
        .all(|e| matches!(e.action, ChangelogAction::Verify));

    if all_verify {
        return Some(SuggestedAction {
            action: ActionKind::SyncModel,
            description: "Model unchanged in recent operations. Consider implementing rather than \
                 further analysis. The current approach may need a concrete attempt."
                .into(),
            example_command: "cog experiment try <target> --kind correction --claim \"...\" \
                 --grounds \"code:<target>\" --desc \"...\""
                .into(),
        });
    }

    // Rule 2: stale evaluated experiment.
    // For each Evaluated experiment, count changelog entries newer than its file mtime.
    // If >= STAGNATION_WINDOW entries accumulated, the experiment was forgotten.
    for exp in active_experiments {
        if exp.status != "evaluated" {
            continue;
        }
        let Some(mtime) = exp.mtime else {
            // Can't determine staleness without mtime — count total entries as fallback
            if changelog.len() >= STAGNATION_WINDOW {
                return Some(SuggestedAction {
                    action: ActionKind::SyncModel,
                    description: format!(
                        "Experiment {} (\"{}\") has been evaluated but not committed/discarded. \
                         Resolve it before starting new work.",
                        exp.short_id, exp.description
                    ),
                    example_command: format!(
                        "cog experiment commit {}  # or: cog experiment discard {}",
                        exp.short_id, exp.short_id
                    ),
                });
            }
            continue;
        };

        let entries_after = changelog.iter().filter(|e| e.timestamp > mtime).count();

        if entries_after >= STAGNATION_WINDOW {
            return Some(SuggestedAction {
                action: ActionKind::SyncModel,
                description: format!(
                    "Experiment {} (\"{}\") has been evaluated but not committed/discarded in {} operations. \
                     Resolve it before starting new work.",
                    exp.short_id, exp.description, entries_after
                ),
                example_command: format!(
                    "cog experiment commit {}  # or: cog experiment discard {}",
                    exp.short_id, exp.short_id
                ),
            });
        }
    }

    None
}
