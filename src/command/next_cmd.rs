use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{NextModelSummary, NextReport, NextSuggestion};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;
use crate::workflow::{ActionKind, WorkflowState, suggest_actions};

/// Label used for each [`ActionKind`] in the suggestions list.
fn action_kind_label(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::InitProject => "init",
        ActionKind::RecordMissingContracts => "model",
        ActionKind::ReviewUncertainAssertions => "review",
        ActionKind::StartRecording => "model",
        ActionKind::AssessImpact => "assess",
        ActionKind::RecordFix => "model",
        ActionKind::VerifyConsistency => "verify",
        ActionKind::StartExperiment => "descent",
        ActionKind::SyncModel => "drift",
        ActionKind::ImplementNow => "descent",
        ActionKind::CommitExperiment => "descent",
        ActionKind::RecoverContext => "recover",
        ActionKind::RecordConstraint => "model",
        ActionKind::ImplementPlanned => "descent",
    }
}

/// Detect all active (Open/Evaluated) experiments from disk.
///
/// Returns a list of [`ActiveExperiment`](crate::domain::ActiveExperiment) sorted
/// by modification time (most recent first).
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

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let raw_status = value["status"].as_str().unwrap_or("").to_string();
        let status_label = match raw_status.as_str() {
            "Evaluated" => "evaluated",
            "Open" => "draft",
            _ => continue, // Committed/Discarded — not active
        };
        let short_id =
            crate::domain::short_id(value["id"].as_str().unwrap_or_default()).to_string();
        let description = value["description"].as_str().unwrap_or("").to_string();

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

/// Execute the `cog next` command.
///
/// Gathers workflow state, model statistics, active experiments, and suggested
/// actions, then formats them into a report.
pub fn execute(
    repo: &dyn Repository,
    wf: &WorkflowState,
    cog_dir: &std::path::Path,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let active_experiments = detect_active_experiments(cog_dir);
    let actions = suggest_actions(wf, repo, &active_experiments);
    let stats = repo.stats().unwrap_or_default();

    // Separate stagnation SyncModel from regular suggestions
    let mut suggestions = Vec::new();
    let mut stagnation_warning = None;
    for a in &actions {
        if matches!(a.action, ActionKind::SyncModel) {
            stagnation_warning = Some(format!(
                "WARNING: {}\n  Next: {}",
                a.description, a.example_command
            ));
        } else {
            suggestions.push(NextSuggestion {
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

    let report = NextReport {
        status: wf.status_label().to_string(),
        phase: wf.phase_label_opt().map(String::from),
        active_experiments,
        model: NextModelSummary {
            entities: stats.entities,
            assertions: stats.assertions,
            active: stats.active_assertions,
            uncertain: stats.uncertain_assertions,
            retracted: stats.retracted_assertions,
        },
        covered: stats.covered_entities,
        coverage_pct,
        suggestions,
        stagnation_warning,
        unresolved_provisional: repo.get_experiment_entity_names().unwrap_or_default(),
    };

    Ok(CommandOutput::success(format::emit_report(&report, output)))
}
