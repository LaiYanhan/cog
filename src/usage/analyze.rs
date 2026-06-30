use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::domain::ChangelogEntry;
use crate::usage::event::UsageEvent;

/// A gap between invocations longer than this starts a new "session".
const SESSION_GAP_MINUTES: i64 = 30;

/// Read and parse all usage events from `<cog_dir>/usage.jsonl`.
/// Returns the events plus a count of corrupt lines that were skipped.
pub fn load_events(cog_dir: &Path) -> (Vec<UsageEvent>, usize) {
    let path = cog_dir.join("usage.jsonl");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return (Vec::new(), 0);
    };
    let mut events = Vec::new();
    let mut skipped = 0;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<UsageEvent>(line) {
            Ok(ev) => events.push(ev),
            Err(_) => skipped += 1,
        }
    }
    (events, skipped)
}

/// Aggregated view of recorded usage, for `cog usage`.
#[derive(Serialize)]
pub struct Summary {
    pub total_invocations: usize,
    pub ok: usize,
    pub errored: usize,
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub sessions: usize,
    pub reads: usize,
    pub writes: usize,
    pub by_command: Vec<(String, usize)>,
    pub phase_transitions: usize,
    pub by_phase_to: Vec<(String, usize)>,
    pub skipped_lines: usize,
    pub mutations_total: usize,
    pub by_action: Vec<(String, usize)>,
}

/// Read-only commands (everything that doesn't mutate the model).
fn is_read(command: &str) -> bool {
    matches!(
        command,
        "query" | "impact" | "trace" | "index" | "stats" | "export" | "next" | "usage"
    )
}

pub fn summarize(events: &[UsageEvent], skipped: usize) -> Summary {
    let mut by_command: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_phase: BTreeMap<String, usize> = BTreeMap::new();
    let mut ok = 0;
    let mut errored = 0;
    let mut reads = 0;
    let mut writes = 0;
    let mut phase_transitions = 0;

    for ev in events {
        *by_command.entry(ev.command.clone()).or_default() += 1;
        if ev.ok {
            ok += 1;
        } else {
            errored += 1;
        }
        if is_read(&ev.command) {
            reads += 1;
        } else {
            writes += 1;
        }
        if ev.phase_to.is_some() {
            phase_transitions += 1;
            if let Some(p) = &ev.phase_to {
                *by_phase.entry(p.clone()).or_default() += 1;
            }
        }
    }

    // Sessions: a gap > SESSION_GAP_MINUTES between consecutive invocations
    // starts a new session. Events are appended in chronological order.
    let mut sessions = if events.is_empty() { 0 } else { 1 };
    for w in events.windows(2) {
        let gap = w[1].ts.signed_duration_since(w[0].ts).num_minutes();
        if gap > SESSION_GAP_MINUTES {
            sessions += 1;
        }
    }

    let mut by_command: Vec<_> = by_command.into_iter().collect();
    by_command.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut by_phase_to: Vec<_> = by_phase.into_iter().collect();
    by_phase_to.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    Summary {
        total_invocations: events.len(),
        ok,
        errored,
        first_ts: events.first().map(|e| e.ts),
        last_ts: events.last().map(|e| e.ts),
        sessions,
        reads,
        writes,
        by_command,
        phase_transitions,
        by_phase_to,
        skipped_lines: skipped,
        mutations_total: 0,
        by_action: Vec::new(),
    }
}

/// Group changelog entries by action (assert/retract/cascade_mark/depend/...),
/// sorted by count descending. The `action` column is structured, so this needs
/// no parsing of the free-text `detail` field.
pub fn changelog_counts(entries: &[ChangelogEntry]) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for e in entries {
        *counts.entry(e.action.to_string()).or_default() += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}
