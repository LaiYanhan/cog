use std::path::Path;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::OutputFormat;
use crate::repo::Repository;
use crate::usage::analyze;

/// `cog usage [--raw]` — summarize local usage statistics.
///
/// Merges two feeds: command invocations from `.cog/usage.jsonl` (reads,
/// writes, sessions, outcomes) and model mutations from the changelog in
/// cog.db (assert/depend/retract/cascade_mark/…). `--raw` dumps raw usage
/// events only.
pub fn execute(
    repo: &dyn Repository,
    cog_dir: &Path,
    raw: bool,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let (events, skipped) = analyze::load_events(cog_dir);

    if raw {
        let mut lines = Vec::with_capacity(events.len());
        for ev in &events {
            lines.push(serde_json::to_string(ev)?);
        }
        return Ok(CommandOutput::success(lines.join("\n")));
    }

    let mut summary = analyze::summarize(&events, skipped);
    let changelog = repo.list_changelog_entries()?;
    summary.mutations_total = changelog.len();
    summary.by_action = analyze::changelog_counts(&changelog);
    let text = match output {
        OutputFormat::Json => serde_json::to_string_pretty(&summary)?,
        OutputFormat::Text => render_text(&summary),
    };
    Ok(CommandOutput::success(text))
}

fn render_text(s: &analyze::Summary) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Usage summary: {} invocations across {} sessions ({} ok, {} errored)",
        s.total_invocations, s.sessions, s.ok, s.errored
    );
    if let (Some(f), Some(l)) = (s.first_ts, s.last_ts) {
        let _ = writeln!(
            out,
            "Span: {} .. {}",
            f.format("%Y-%m-%d %H:%M"),
            l.format("%Y-%m-%d %H:%M")
        );
    }
    let _ = writeln!(
        out,
        "Reads: {} | Writes: {} | Phase transitions: {} | Skipped log lines: {}",
        s.reads, s.writes, s.phase_transitions, s.skipped_lines
    );
    if s.total_invocations == 0 && s.mutations_total == 0 {
        let _ = writeln!(out, "\n(No usage recorded yet.)");
        return out;
    }
    if s.total_invocations > 0 {
        let _ = writeln!(out, "\nBy command:");
        for (cmd, n) in &s.by_command {
            let _ = writeln!(out, "  {cmd:<14} {n}");
        }
        if !s.by_phase_to.is_empty() {
            let _ = writeln!(out, "\nBy resulting phase:");
            for (p, n) in &s.by_phase_to {
                let _ = writeln!(out, "  {p:<16} {n}");
            }
        }
    }
    if s.mutations_total > 0 {
        let _ = writeln!(
            out,
            "\nBy mutation (changelog, {} total):",
            s.mutations_total
        );
        for (a, n) in &s.by_action {
            let _ = writeln!(out, "  {a:<14} {n}");
        }
    }
    out
}
