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
    VerifyConsistency,
    StartExperiment,
    SyncModel,
    ImplementNow,
    CommitExperiment,
    RecoverContext,
    RecordConstraint,
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

// ── Phase-specific suggestion functions ──────────────────────────────────────

fn suggest_fresh_scan(repo: &dyn Repository, stats: &ModelStats) -> Vec<SuggestedAction> {
    let mut actions = Vec::new();
    actions.push(SuggestedAction {
        action: ActionKind::StartRecording,
        description: format!(
            "{} entities found. Start recording assertions.",
            stats.entities
        ),
        example_command: "cog query <core_entity>".into(),
    });
    let orphans = count_orphan_entities(repo, stats);
    if orphans > 0 {
        actions.push(SuggestedAction {
            action: ActionKind::RecordMissingContracts,
            description: format!("{orphans} entities have no assertions yet."),
            example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
        });
    }
    actions
}

fn suggest_exploring(
    repo: &dyn Repository,
    stats: &ModelStats,
    coverage_pct: f64,
) -> Vec<SuggestedAction> {
    let mut actions = Vec::new();
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
            example_command:
                "cog experiment try <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\""
                    .into(),
        });
    } else if coverage_pct > COVERAGE_IMPLEMENT_THRESHOLD {
        actions.push(SuggestedAction {
            action: ActionKind::ImplementNow,
            description: format!(
                "Coverage is {:.0}%. Consider a sandbox experiment before implementing.",
                coverage_pct
            ),
            example_command:
                "cog experiment try <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\""
                    .into(),
        });
    } else {
        let orphans = count_orphan_entities(repo, stats);
        if orphans > 0 {
            actions.push(SuggestedAction {
                action: ActionKind::RecordMissingContracts,
                description: format!("{orphans} entities have no assertions yet."),
                example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
            });
        }
    }
    actions
}

fn suggest_post_change() -> Vec<SuggestedAction> {
    vec![SuggestedAction {
        action: ActionKind::RecordFix,
        description: "Code changed. Record corrections for changed entities.".into(),
        example_command:
            "cog assert <entity> --kind correction --claim \"...\" --grounds \"code:<entity>\""
                .into(),
    }]
}
fn suggest_debugging(stats: &ModelStats, changelog: &[ChangelogEntry]) -> Vec<SuggestedAction> {
    let mut actions = Vec::new();

    // ── Correction-based context recovery ─────────────────────────────
    // Extract entity names from recent correction assertions in the changelog.
    let mut correction_entities: Vec<String> = Vec::new();
    for entry in changelog.iter().rev() {
        if entry.action == ChangelogAction::Assert
            && entry.detail.contains("kind=correction")
            && let Some(name) = entry.detail.strip_prefix("entity=")
        {
            let name = match name.find(" kind=") {
                Some(end) => &name[..end],
                None => name,
            };
            if !correction_entities.contains(&name.to_string()) {
                correction_entities.push(name.to_string());
            }
            if correction_entities.len() >= 5 {
                break;
            }
        }
    }

    if !correction_entities.is_empty() {
        let sample = correction_entities.join(", ");
        let first = &correction_entities[0];
        actions.push(SuggestedAction {
            action: ActionKind::RecoverContext,
            description: format!("Recent corrections on: {}", sample),
            example_command: format!("cog query {}", first),
        });
    }

    // ── Retracted assertions ──────────────────────────────────────────
    if stats.retracted_assertions > 0 {
        actions.push(SuggestedAction {
            action: ActionKind::ReviewUncertainAssertions,
            description: format!(
                "{} assertion(s) retracted — verify that knowledge is current.",
                stats.retracted_assertions
            ),
            example_command: "cog query <recently_modified_entity>".into(),
        });
    }

    // ── Uncertain assertions (cascade fallout) ────────────────────────
    if stats.uncertain_assertions > 0 && stats.retracted_assertions == 0 {
        actions.push(SuggestedAction {
            action: ActionKind::ReviewUncertainAssertions,
            description: format!(
                "{} assertions are uncertain since last retraction.",
                stats.uncertain_assertions
            ),
            example_command: "cog query <affected_entity>".into(),
        });
    }

    // ── Fallback: no model signals to guide recovery ──────────────────

    // ── Constraint capture: record root cause as invariant ────────────
    actions.push(SuggestedAction {
        action: ActionKind::RecordConstraint,
        description: "If you fixed a bug, record the constraint it violated as an invariant or fragility assertion.".into(),
        example_command: "cog assert <entity> --kind invariant --claim \"<what constraint was violated>\" --grounds \"issue:<id>\"".into(),
    });
    if actions.is_empty() {
        actions.push(SuggestedAction {
            action: ActionKind::RecoverContext,
            description: "Read requirements, identify remaining gaps, and continue implementation."
                .into(),
            example_command: "cog impact <core_entity>".into(),
        });
    }

    actions
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
    let phase_actions = match phase {
        WorkflowPhase::FreshScan => suggest_fresh_scan(repo, &stats),
        WorkflowPhase::Debugging => suggest_debugging(&stats, &changelog),
        WorkflowPhase::Exploring => suggest_exploring(repo, &stats, coverage_pct),
        WorkflowPhase::PostChange => suggest_post_change(),
    };
    actions.extend(phase_actions);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChangelogAction;
    use chrono::Utc;

    fn entry(action: ChangelogAction, detail: &str) -> ChangelogEntry {
        ChangelogEntry {
            id: "test".into(),
            action,
            target_id: "test".into(),
            detail: detail.into(),
            timestamp: Utc::now(),
        }
    }

    fn empty_stats() -> ModelStats {
        ModelStats::default()
    }

    // ── Correction recovery ────────────────────────────────────────────

    #[test]
    fn debug_with_corrections_extracts_entity_names() {
        let changelog = vec![
            entry(
                ChangelogAction::Assert,
                "entity=foo::bar kind=correction claim=fix",
            ),
            entry(
                ChangelogAction::Assert,
                "entity=foo::baz kind=correction claim=fix2",
            ),
        ];
        let actions = suggest_debugging(&empty_stats(), &changelog);

        let recover = actions
            .iter()
            .find(|a| matches!(a.action, ActionKind::RecoverContext));
        assert!(recover.is_some(), "should have a RecoverContext action");
        let r = recover.unwrap();
        assert!(
            r.description.contains("foo::bar"),
            "should mention first entity"
        );
        assert!(
            r.description.contains("foo::baz"),
            "should mention second entity"
        );
        // Changelog is iterated in reverse, so foo::baz (last entry) is first discovered
        assert_eq!(r.example_command, "cog query foo::baz");
    }

    #[test]
    fn debug_corrections_dedup_entity_names() {
        let changelog = vec![
            entry(
                ChangelogAction::Assert,
                "entity=alpha kind=correction claim=a",
            ),
            entry(
                ChangelogAction::Assert,
                "entity=alpha kind=correction claim=b",
            ),
        ];
        let actions = suggest_debugging(&empty_stats(), &changelog);

        let recover = actions
            .iter()
            .find(|a| matches!(a.action, ActionKind::RecoverContext))
            .unwrap();
        // "alpha" should appear exactly once (deduped), but the join includes it once
        assert_eq!(recover.description.matches("alpha").count(), 1);
    }

    #[test]
    fn debug_corrections_caps_at_five() {
        let changelog: Vec<ChangelogEntry> = (0..10)
            .map(|i| {
                entry(
                    ChangelogAction::Assert,
                    &format!("entity=e{i} kind=correction claim=x"),
                )
            })
            .collect();
        let actions = suggest_debugging(&empty_stats(), &changelog);

        let recover = actions
            .iter()
            .find(|a| matches!(a.action, ActionKind::RecoverContext))
            .unwrap();
        // The description lists entity names joined by ", "
        let names_part = recover
            .description
            .strip_prefix("Recent corrections on: ")
            .unwrap();
        assert_eq!(names_part.split(", ").count(), 5);
    }

    #[test]
    fn debug_ignores_non_correction_asserts() {
        let changelog = vec![
            entry(ChangelogAction::Assert, "entity=foo kind=contract claim=x"),
            entry(ChangelogAction::Assert, "entity=bar kind=invariant claim=y"),
        ];
        let actions = suggest_debugging(&empty_stats(), &changelog);

        // No corrections, no retractions, no uncertain → only RecordConstraint
        assert!(
            actions
                .iter()
                .any(|a| matches!(a.action, ActionKind::RecordConstraint))
        );
        assert!(!actions[0].description.contains("foo"));
    }

    // ── Retraction path ────────────────────────────────────────────────

    #[test]
    fn debug_with_retracted_shows_review() {
        let stats = ModelStats {
            retracted_assertions: 3,
            ..ModelStats::default()
        };
        let actions = suggest_debugging(&stats, &[]);

        let review = actions
            .iter()
            .find(|a| matches!(a.action, ActionKind::ReviewUncertainAssertions));
        assert!(review.is_some(), "should have review action");
        assert!(review.unwrap().description.contains("3"));
    }

    #[test]
    fn debug_retraction_suppresses_uncertain_path() {
        // When retracted_assertions > 0, the uncertain-only path should NOT fire
        let stats = ModelStats {
            retracted_assertions: 1,
            uncertain_assertions: 5,
            ..ModelStats::default()
        };
        let actions = suggest_debugging(&stats, &[]);

        let review_count = actions
            .iter()
            .filter(|a| matches!(a.action, ActionKind::ReviewUncertainAssertions))
            .count();
        assert_eq!(
            review_count, 1,
            "should have exactly one review (retracted), not two"
        );
    }

    // ── Uncertain-only path ────────────────────────────────────────────

    #[test]
    fn debug_uncertain_only_when_no_retractions() {
        let stats = ModelStats {
            uncertain_assertions: 7,
            ..ModelStats::default()
        };
        let actions = suggest_debugging(&stats, &[]);

        let review = actions
            .iter()
            .find(|a| matches!(a.action, ActionKind::ReviewUncertainAssertions));
        assert!(review.is_some());
        assert!(review.unwrap().description.contains("7"));
        assert!(review.unwrap().description.contains("uncertain"));
    }

    // ── Fallback ───────────────────────────────────────────────────────

    #[test]
    fn debug_fallback_when_no_signals() {
        let actions = suggest_debugging(&empty_stats(), &[]);
        // RecordConstraint is always present; RecoverContext no longer needed as fallback
        assert!(
            actions
                .iter()
                .any(|a| matches!(a.action, ActionKind::RecordConstraint))
        );
        assert!(actions[0].description.contains("constraint"));
    }

    // ── No generic trace/verify ────────────────────────────────────────

    #[test]
    fn debug_never_suggests_trace_or_verify() {
        // Test all signal combinations
        let cases: Vec<(ModelStats, Vec<ChangelogEntry>)> = vec![
            (empty_stats(), vec![]),
            (
                ModelStats {
                    retracted_assertions: 2,
                    ..empty_stats()
                },
                vec![entry(
                    ChangelogAction::Assert,
                    "entity=x kind=correction claim=c",
                )],
            ),
            (
                ModelStats {
                    uncertain_assertions: 10,
                    ..empty_stats()
                },
                vec![],
            ),
        ];

        for (stats, changelog) in cases {
            let actions = suggest_debugging(&stats, &changelog);
            for a in &actions {
                assert!(
                    !matches!(a.action, ActionKind::VerifyConsistency),
                    "debugging should never suggest VerifyConsistency, got {:?}",
                    a.action
                );
            }
        }
    }

    // ── Correction + retraction combined ───────────────────────────────

    #[test]
    fn debug_correction_and_retraction_both_present() {
        let stats = ModelStats {
            retracted_assertions: 2,
            ..ModelStats::default()
        };
        let changelog = vec![entry(
            ChangelogAction::Assert,
            "entity=core::fn kind=correction claim=fix",
        )];
        let actions = suggest_debugging(&stats, &changelog);

        assert!(
            actions
                .iter()
                .any(|a| matches!(a.action, ActionKind::RecoverContext))
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a.action, ActionKind::ReviewUncertainAssertions))
        );
        assert!(actions.len() >= 2);
    }
}
