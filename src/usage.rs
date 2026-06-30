//! Local usage logging — answers "is the cognitive layer actually being used?"
//!
//! Records one event per `cog` invocation to `.cog/usage.jsonl` (local only,
//! never transmitted; disable with `COG_USAGE=off`). The write-side feed is the
//! existing `changelog` in cog.db; this module covers reads + command context
//! that the changelog deliberately omits.

pub mod analyze;
pub mod event;
pub mod recorder;

pub use event::UsageEvent;
