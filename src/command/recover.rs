use crate::command::CommandOutput;
use crate::domain::{AssertionStatus, short_id};
use crate::format::OutputFormat;
use crate::repo::Repository;
use anyhow::Result;
use std::fmt::Write;

/// Recover Uncertain assertions whose dependencies are all Active again.
/// Lists recoverable assertions and, if --apply is set, restores them to Active.
pub fn execute(repo: &dyn Repository, apply: bool, _output: OutputFormat) -> Result<CommandOutput> {
    let all = repo.list_assertions()?;
    let uncertain: Vec<_> = all
        .iter()
        .filter(|a| a.status == AssertionStatus::Uncertain)
        .collect();

    if uncertain.is_empty() {
        return Ok(CommandOutput::success(
            "No uncertain assertions to recover.",
        ));
    }

    let mut recoverable = Vec::new();
    let mut blocked = Vec::new();

    for assertion in &uncertain {
        let deps = repo.get_dependencies(&assertion.id)?;
        let all_active = deps.iter().all(|d| d.status == AssertionStatus::Active);
        if all_active {
            recoverable.push((*assertion).clone());
        } else {
            let inactive_count = deps
                .iter()
                .filter(|d| d.status != AssertionStatus::Active)
                .count();
            blocked.push(((*assertion).clone(), inactive_count));
        }
    }

    // Build output
    let mut text = String::new();

    if !recoverable.is_empty() {
        if apply {
            for a in &recoverable {
                repo.update_assertion_status(&a.id, AssertionStatus::Active)?;
            }
            let _ = writeln!(text, "Recovered {} assertion(s):", recoverable.len());
        } else {
            let _ = writeln!(
                text,
                "{} assertion(s) eligible for recovery:",
                recoverable.len()
            );
        }
        for a in &recoverable {
            let _ = writeln!(text, "  [{}] {} — \"{}\"", short_id(&a.id), a.kind, a.claim);
        }
        if !apply {
            let _ = writeln!(text, "\nApply with: cog recover --apply");
        }
    }

    if !blocked.is_empty() {
        let _ = writeln!(text);
        let _ = writeln!(
            text,
            "{} uncertain assertion(s) still blocked:",
            blocked.len()
        );
        for (a, count) in &blocked {
            let _ = writeln!(
                text,
                "  [{}] {} — \"{}\" ({} inactive dep(s))",
                short_id(&a.id),
                a.kind,
                a.claim,
                count
            );
        }
    }

    Ok(CommandOutput::success(text))
}
