use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single usage event — one line in `.cog/usage.jsonl`.
///
/// One event per `cog` invocation. Records *what* ran and the outcome so the
/// project owner can answer "is the cognitive layer actually being used?"
/// Local only; never transmitted. Disable with `COG_USAGE=off`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageEvent {
    /// When the command ran.
    pub ts: DateTime<Utc>,
    /// Command verb (e.g. "assert", "sync").
    pub command: String,
    /// Whether the command succeeded.
    pub ok: bool,
    /// Exit code (None if the command errored before producing output).
    pub exit_code: Option<i32>,
    /// Wall-clock duration of the command, in milliseconds.
    pub duration_ms: u64,
    /// Whether sync detected code drift.
    pub has_drift: bool,
    /// Workflow phase before the command (present only if it changed).
    pub phase_from: Option<String>,
    /// Workflow phase after the command (present only if it changed).
    pub phase_to: Option<String>,
    /// Structured args — entity refs, IDs, kinds, flags. Never free-text prose
    /// (claims/reasons live in cog.db, referenced by ID).
    pub args: Value,
    /// Optional structured payload attached by the command (e.g. sync relation
    /// breakdown). None for most commands.
    pub metrics: Option<Value>,
}
