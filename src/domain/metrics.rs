use clap::ValueEnum;
use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

/// Metadata about an entity extracted during tree-sitter scanning.
///
/// Stored alongside the entity and updated after each scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EntityMetrics {
    /// Lines of code (end_line - start_line + 1) from tree-sitter node range.
    pub line_count: Option<u32>,
    /// Number of entities that depend on this one (computed post-scan via BFS).
    pub fan_in: Option<u32>,
    /// Number of entities this one depends on (computed post-scan via BFS).
    pub fan_out: Option<u32>,
    /// Visibility of the symbol.
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Private,
    Public,
    /// Rust-specific: `pub(crate)`, `pub(super)`, etc.
    Restricted,
}

impl Visibility {
    /// Whether this visibility is public. Used by risk assessment and display filtering.
    pub fn is_public(self) -> bool {
        matches!(self, Visibility::Public)
    }
}

impl Display for Visibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
            Visibility::Restricted => write!(f, "restricted"),
        }
    }
}
impl EntityMetrics {
    /// A zero-metrics instance used for manually created entities.
    pub fn empty() -> Self {
        Self::default()
    }
}
