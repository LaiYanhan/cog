use std::io::Write;
use std::path::Path;

use crate::usage::event::UsageEvent;

/// Append `event` as one JSON line to `<cog_dir>/usage.jsonl`.
///
/// Best-effort: any I/O or serialization failure is logged to stderr and
/// swallowed — recording usage must never break the command that triggered it.
/// Honors `COG_USAGE=off|0|false` to disable recording entirely.
pub fn record(cog_dir: &Path, event: &UsageEvent) {
    if disabled() {
        return;
    }
    let line = match serde_json::to_string(event) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("warning: failed to serialize usage event: {e}");
            return;
        }
    };
    let path = cog_dir.join("usage.jsonl");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut f) => {
            if let Err(e) = writeln!(f, "{line}") {
                eprintln!("warning: failed to write usage log {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!(
            "warning: failed to open usage log {} for append: {e}",
            path.display()
        ),
    }
}

fn disabled() -> bool {
    matches!(
        std::env::var("COG_USAGE").ok().as_deref(),
        Some("off") | Some("0") | Some("false")
    )
}
