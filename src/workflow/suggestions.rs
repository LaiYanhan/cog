use crate::domain::ModelStats;
use crate::repo::Repository;
use crate::workflow::state::WorkflowPhase;

use super::state::WorkflowState;

// ── Suggested action types ──

/// A suggestion the CLI offers to the agent — "what you can do next".
#[derive(Debug, Clone)]
pub struct SuggestedAction {
    pub action: ActionKind,
    pub description: String,
    pub why: String,
    pub example_command: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ActionKind {
    InitProject,
    RecordMissingContracts { entity_count: usize },
    ReviewUncertainAssertions { count: usize },
    StartRecording,
    AssessImpact { entity: String },
    StartChange,
    VerifyChanges,
    RecordFix { entity: String },
    FinishChange,
    AbortChange,
    TraceRootCause,
    VerifyConsistency,
    StartExperiment,
    StartExperimentDuringChange,
}

// ── Suggestion engine ──

/// Returns suggested actions based on current workflow state and model data.
pub fn suggest_actions(state: &WorkflowState, repo: &dyn Repository) -> Vec<SuggestedAction> {
    match state {
        WorkflowState::Uninit => vec![SuggestedAction {
            action: ActionKind::InitProject,
            description: "No cognitive model found. Run init to scan the codebase.".into(),
            why: "Without a structural model, cog cannot provide guidance.".into(),
            example_command: "cog init .".into(),
        }],
        WorkflowState::Ready { phase } => suggest_for_ready(phase, repo),
        WorkflowState::Changing {
            description,
            affected_entities,
            ..
        } => suggest_for_changing(description, affected_entities, repo),
    }
}

fn suggest_for_ready(phase: &WorkflowPhase, repo: &dyn Repository) -> Vec<SuggestedAction> {
    let mut actions = Vec::new();
    let stats = repo.stats().unwrap_or_else(|_| ModelStats::default());

    match phase {
        WorkflowPhase::FreshScan => {
            if stats.assertions == 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::StartRecording,
                    description: format!("{} entities found but 0 assertions.", stats.entities),
                    why: "Entities without assertions are 'unknown unknowns' during changes."
                        .into(),
                    example_command: "cog query <core_entity>".into(),
                });
            }
            let orphans = count_orphan_entities(repo, &stats);
            if orphans > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::RecordMissingContracts {
                        entity_count: orphans,
                    },
                    description: format!("{orphans} entities have no assertions yet."),
                    why: "Core modules need contracts before any change.".into(),
                    example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
                });
            }
        }
        WorkflowPhase::Exploring => {
            let orphans = count_orphan_entities(repo, &stats);
            if orphans > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::RecordMissingContracts {
                        entity_count: orphans,
                    },
                    description: format!("{orphans} entities have no assertions yet."),
                    why: "Core modules need contracts before any change.".into(),
                    example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
                });
            }
            actions.push(SuggestedAction {
                action: ActionKind::AssessImpact {
                    entity: "try a core entity".into(),
                },
                description: "Run impact analysis to understand downstream dependencies.".into(),
                why: "Knowing blast radius before changes reduces surprise.".into(),
                example_command: "cog impact <core_entity>".into(),
            });
        }
        WorkflowPhase::Assessing => {
            actions.push(SuggestedAction {
                action: ActionKind::StartChange,
                description: "Begin a code change now that you've assessed impact.".into(),
                why: "Impact assessment is most useful just before making a change.".into(),
                example_command: "cog start-change \"<description>\"".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::StartExperiment,
                description: "Or, run a what-if experiment before committing to a change.".into(),
                why: "Experiments let you test hypotheses without modifying the codebase.".into(),
                example_command: "cog experiment start <entity>".into(),
            });
        }
        WorkflowPhase::PostChange => {
            actions.push(SuggestedAction {
                action: ActionKind::RecordFix {
                    entity: "changed entity".into(),
                },
                description: "Record corrections for changed entities.".into(),
                why: "Keep the model in sync with the code after changes.".into(),
                example_command: "cog assert <entity> --kind correction --claim \"...\"".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::StartChange,
                description: "Begin another change cycle if more work remains.".into(),
                why: "Ready for the next iteration.".into(),
                example_command: "cog start-change \"<description>\"".into(),
            });
        }
        WorkflowPhase::Debugging => {
            if stats.uncertain_assertions > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::ReviewUncertainAssertions {
                        count: stats.uncertain_assertions as usize,
                    },
                    description: format!(
                        "{} assertions are uncertain since last retraction.",
                        stats.uncertain_assertions
                    ),
                    why: "Uncertain assertions have weakened ground — they need re-verification."
                        .into(),
                    example_command: "cog query <affected_entity>".into(),
                });
            }
            actions.push(SuggestedAction {
                action: ActionKind::TraceRootCause,
                description: "Trace dependency chains to find root causes.".into(),
                why: "TMS cascade may have weakened downstream assertions.".into(),
                example_command: "cog trace <entity>".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::VerifyConsistency,
                description: "Run verify to check structural consistency.".into(),
                why: "Retraction may have left orphaned dependencies.".into(),
                example_command: "cog verify --scan".into(),
            });
        }
    }

    // Ready states can always enter change mode
    actions.push(SuggestedAction {
        action: ActionKind::StartChange,
        description: "Begin a code change with impact assessment.".into(),
        why: "Always assess impact before modifying code.".into(),
        example_command: "cog start-change \"<description>\"".into(),
    });

    actions
}

fn suggest_for_changing(
    description: &str,
    affected: &[String],
    _repo: &dyn Repository,
) -> Vec<SuggestedAction> {
    let mut actions = vec![SuggestedAction {
        action: ActionKind::VerifyChanges,
        description: format!("Verify model consistency after: {description}"),
        why: "Changes may violate existing contracts or invariants.".into(),
        example_command: "cog verify".into(),
    }];

    if let Some(first) = affected.first() {
        actions.push(SuggestedAction {
            action: ActionKind::RecordFix {
                entity: first.clone(),
            },
            description: format!("Record fix for {first}"),
            why: "Document corrections to keep the model accurate.".into(),
            example_command: format!("cog assert {first} --kind correction --claim \"...\""),
        });
    }

    actions.push(SuggestedAction {
        action: ActionKind::StartExperimentDuringChange,
        description: "Run a what-if experiment to test a fix before committing.".into(),
        why: "Experiments let you verify hypotheses without modifying the real model.".into(),
        example_command: "cog experiment start <affected_entity>".into(),
    });
    actions.push(SuggestedAction {
        action: ActionKind::FinishChange,
        description: "Finish this change cycle.".into(),
        why: "All fixes recorded. Return to normal operation.".into(),
        example_command: "cog finish-change".into(),
    });
    actions.push(SuggestedAction {
        action: ActionKind::AbortChange,
        description: "Abort this change and discard tracking.".into(),
        why: "You can always abandon a change cycle and return to Ready state.".into(),
        example_command: "cog abort-change".into(),
    });

    actions
}

/// Count entities with zero assertions. Uses assertion count from stats
/// as a fast check — if assertions == entities, no orphans. Otherwise
/// queries the repo.
fn count_orphan_entities(repo: &dyn Repository, stats: &ModelStats) -> usize {
    if stats.assertions == 0 {
        return stats.entities as usize;
    }
    // Use the existing method — scan entity names vs assertion entity_ids
    repo.count_unasserted_entities().unwrap_or(0) as usize
}
